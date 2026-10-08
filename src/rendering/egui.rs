#![allow(dead_code)]

use std::{cell::RefCell, rc::Rc, sync::Arc};

use egui::{FullOutput, ImmediateViewport, OrderedViewportIdMap, RawInput, ViewportBuilder, ViewportId, ViewportIdMap, ViewportIdSet, ViewportInfo, ViewportOutput};
use egui_wgpu::{Renderer, ScreenDescriptor};
use egui_winit::winit;
use wgpu::{TextureView, CommandEncoder};
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::{Icon, Theme, Window, WindowId}};

use crate::{console_error, rendering::{gpu_timer::{GpuTimer, GpuTimerPass}, wgpu::WGpu}};

pub struct EGui
{
    pub ctx: egui::Context,
    pub ui_state: egui_winit::State,
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,

    pub output: Option<FullOutput>,

    // renderer + windows of the other viewports (shared with the immediate viewport renderer)
    pub viewports: Rc<RefCell<EGuiViewports>>,

    // viewports as native windows - drags between them need window positions (not on wasm/wayland)
    pub multi_viewport_support: bool,

    // gpu time of the egui render pass (None if the adapter does not support timestamp queries)
    gpu_timer: Option<GpuTimer>,
}

impl EGui
{
    pub fn new(wgpu: &WGpu, window: Arc<winit::window::Window>) -> Self
    {
        let device = wgpu.device();
        let size = window.inner_size();

        let ctx: egui::Context = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let viewport_id = ctx.viewport_id();

        let native_pixels_per_point = window.scale_factor() as f32;
        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let theme = Some(winit::window::Theme::Dark);
        let ui_state = egui_winit::State::new(ctx.clone(), viewport_id, &window, Some(native_pixels_per_point), theme, Some(max_texture_side));

        let renderer = Renderer::new(&device, wgpu.surface_config().format, egui_wgpu::RendererOptions
        {
            dithering: true,
            ..Default::default()
        });

        let viewports = Rc::new(RefCell::new(EGuiViewports::new(renderer, device.clone(), wgpu.queue_mut().clone(), max_texture_side)));

        let multi_viewport_support = cfg!(not(target_arch = "wasm32")) && window.inner_position().is_ok();
        ctx.set_embed_viewports(!multi_viewport_support);

        if multi_viewport_support
        {
            install_immediate_viewport_renderer(&viewports);
        }

        Self
        {
            ctx: ctx,
            ui_state: ui_state,
            screen_descriptor: ScreenDescriptor
            {
                pixels_per_point: window.scale_factor() as f32,
                size_in_pixels: [size.width, size.height],
            },
            output: None,

            viewports,

            multi_viewport_support,

            gpu_timer: GpuTimer::new(device),
        }
    }

    // input of the main window - with the positions of all windows (drags between them)
    pub fn take_input(&mut self, window: &Window) -> egui::RawInput
    {
        if !self.multi_viewport_support
        {
            return self.ui_state.take_egui_input(window);
        }

        let viewports = &mut *self.viewports.borrow_mut();
        viewports.update_infos(&self.ctx, window);

        take_viewport_input(&mut viewports.infos, ViewportId::ROOT, &mut self.ui_state, window)
    }

    // accumulates the texture delta of the current frame so it survives a skipped render
    // (e.g. when wgpu.start_render() returns None during resize/surface reconfigure)
    pub fn set_output(&mut self, mut output: FullOutput, window: &Window)
    {
        let delta = std::mem::take(&mut output.textures_delta);

        let viewports = &mut *self.viewports.borrow_mut();
        viewports.pending_textures_delta.append(delta);
        viewports.handle_viewport_output(&self.ctx, window, &output.viewport_output);

        self.output = Some(output);
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) -> Vec<egui::ClippedPrimitive>
    {
        let output = self.output.clone().unwrap();
        let clipped_primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        let viewports = &mut *self.viewports.borrow_mut();
        viewports.renderer.update_buffers(device, queue, encoder, &clipped_primitives, &self.screen_descriptor);

        viewports.apply_pending_textures();

        clipped_primitives
    }

    // play mode: the main window paints no ui, but pending uploads/frees must not pile up
    pub fn flush_textures(&mut self)
    {
        self.viewports.borrow_mut().apply_pending_textures();
    }

    pub fn resize(&mut self, width: u32, height: u32, scale_factor: Option<f64>)
    {
        self.screen_descriptor.size_in_pixels[0] = width;
        self.screen_descriptor.size_in_pixels[1] = height;

        if scale_factor.is_some()
        {
            self.screen_descriptor.pixels_per_point = scale_factor.unwrap() as f32;
        }
    }

    pub fn on_event(&mut self, event: &winit::event::WindowEvent, window: Arc<winit::window::Window>) -> bool
    {
        self.ui_state.on_window_event(&window, event).consumed
    }

    pub fn on_viewport_window_event(&mut self, window_id: WindowId, event: &WindowEvent)
    {
        self.viewports.borrow_mut().on_window_event(window_id, event);
    }

    pub fn has_pending_viewport_windows(&self) -> bool
    {
        self.viewports.borrow().has_pending_windows()
    }

    pub fn create_pending_viewport_windows(&mut self, event_loop: &ActiveEventLoop, wgpu: &WGpu, icon: Option<Icon>)
    {
        self.viewports.borrow_mut().create_pending_windows(&self.ctx, event_loop, wgpu, icon);
    }

    pub fn request_repaint(&self)
    {
        self.ctx.request_repaint();
    }

    pub fn render(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        // read back the gpu timing of the previous egui pass
        if let Some(gpu_timer) = self.gpu_timer.as_mut()
        {
            gpu_timer.read_back_results(wgpu);
        }

        let primitives = self.prepare(wgpu.device(), wgpu.queue_mut(), encoder);

        {
            let timer_segment = self.gpu_timer.as_mut().and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Egui));

            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: None,
                color_attachments:
                &[
                    Some(wgpu::RenderPassColorAttachment
                    {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations
                        {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: timer_segment.map(|timer_segment| timer_segment.full_render_writes()),
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // forget_lifetime is intentional -> see render description
            // https://github.com/emilk/egui/pull/5149
            self.viewports.borrow().renderer.render(&mut pass.forget_lifetime(), &primitives, &self.screen_descriptor);
        }

        // resolve the gpu timestamps into the readback buffer (read back in the next frame)
        if let Some(gpu_timer) = self.gpu_timer.as_mut()
        {
            gpu_timer.resolve(encoder);
        }
    }

    // averaged gpu time of the egui render pass in ms
    pub fn gpu_render_time(&self) -> Option<f32>
    {
        self.gpu_timer.as_ref().and_then(|gpu_timer| gpu_timer.pass_times().egui)
    }
}

// native window of an egui viewport (without integration support egui embeds viewports as egui::Window into the main window)
pub struct ViewportWindow
{
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    ui_state: egui_winit::State,
}

impl ViewportWindow
{
    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32)
    {
        if width == 0 || height == 0
        {
            return;
        }

        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(device, &self.surface_config);
    }
}

// shared by all egui windows - also used by the immediate viewport renderer, which runs in the middle of Context::run_ui
pub struct EGuiViewports
{
    // one renderer for all windows (same context -> same texture ids), each window needs its own submit (shared vertex/uniform buffers)
    pub renderer: egui_wgpu::Renderer,

    // texture changes not uploaded yet (skipped frames, viewports without a window yet)
    pub pending_textures_delta: egui::TexturesDelta,

    pub windows: ViewportIdMap<ViewportWindow>,
    pub infos: ViewportIdMap<ViewportInfo>,

    // winit can only create windows with the active event loop -> created after the frame (see create_pending_windows)
    pending_windows: Vec<(ViewportId, ViewportBuilder)>,
    failed_windows: ViewportIdSet,

    device: wgpu::Device,
    queue: wgpu::Queue,
    max_texture_side: usize,
}

impl Drop for EGuiViewports
{
    fn drop(&mut self)
    {
        // shutdown: unapplied deltas don't matter anymore (egui asserts on dropping a non-empty delta)
        self.pending_textures_delta.clear();
    }
}

impl EGuiViewports
{
    pub fn new(renderer: egui_wgpu::Renderer, device: wgpu::Device, queue: wgpu::Queue, max_texture_side: usize) -> Self
    {
        Self
        {
            renderer,
            pending_textures_delta: egui::TexturesDelta::default(),

            windows: ViewportIdMap::default(),
            infos: ViewportIdMap::default(),

            pending_windows: vec![],
            failed_windows: ViewportIdSet::default(),

            device,
            queue,
            max_texture_side,
        }
    }

    // uploads and frees all pending texture changes - call only after every window of this frame is rendered
    pub fn apply_pending_textures(&mut self)
    {
        let mut textures_delta = std::mem::take(&mut self.pending_textures_delta);

        for (tex_id, img_deltas) in &textures_delta.set
        {
            for img_delta in img_deltas
            {
                self.renderer.update_texture(&self.device, &self.queue, *tex_id, img_delta);
            }
        }

        for tex_id in &textures_delta.free
        {
            self.renderer.free_texture(tex_id);
        }

        textures_delta.clear();
    }

    // window positions/sizes of all viewports - each pass gets all of them to map positions between the windows
    pub fn update_infos(&mut self, ctx: &egui::Context, main_window: &Window)
    {
        egui_winit::update_viewport_info(self.infos.entry(ViewportId::ROOT).or_default(), ctx, main_window, false);

        for (viewport_id, viewport) in &self.windows
        {
            egui_winit::update_viewport_info(self.infos.entry(*viewport_id).or_default(), ctx, &viewport.window, false);
        }
    }

    pub fn has_pending_windows(&self) -> bool
    {
        !self.pending_windows.is_empty()
    }

    pub fn create_pending_windows(&mut self, ctx: &egui::Context, event_loop: &ActiveEventLoop, wgpu: &WGpu, icon: Option<Icon>)
    {
        for (viewport_id, builder) in std::mem::take(&mut self.pending_windows)
        {
            let window = match egui_winit::create_window(ctx, event_loop, &builder)
            {
                Ok(window) => Arc::new(window),
                Err(err) =>
                {
                    console_error!(format!("failed to create the window for viewport {:?}: {:?}", viewport_id, err));
                    self.failed_windows.insert(viewport_id);
                    continue;
                }
            };

            window.set_window_icon(icon.clone());

            let Some((surface, surface_config)) = wgpu.create_window_surface(window.clone()) else
            {
                self.failed_windows.insert(viewport_id);
                continue;
            };

            let ui_state = egui_winit::State::new(ctx.clone(), viewport_id, &window, Some(window.scale_factor() as f32), Some(Theme::Dark), Some(self.max_texture_side));

            egui_winit::update_viewport_info(self.infos.entry(viewport_id).or_default(), ctx, &window, true);

            self.windows.insert(viewport_id, ViewportWindow { window, surface, surface_config, ui_state });
        }
    }

    pub fn on_window_event(&mut self, window_id: WindowId, event: &WindowEvent)
    {
        let EGuiViewports { windows, infos, device, .. } = self;

        let Some((viewport_id, viewport)) = windows.iter_mut().find(|(_, viewport)| viewport.window.id() == window_id) else
        {
            return;
        };

        match event
        {
            // the ui decides what happens (egui: close_requested)
            WindowEvent::CloseRequested => infos.entry(*viewport_id).or_default().events.push(egui::ViewportEvent::Close),
            WindowEvent::Resized(size) => viewport.resize(device, size.width, size.height),
            _ => {}
        }

        _ = viewport.ui_state.on_window_event(&viewport.window, event);
    }

    // output of the outermost pass: it has the commands of all viewports and tells which ones still exist
    pub fn handle_viewport_output(&mut self, ctx: &egui::Context, main_window: &Window, viewport_output: &OrderedViewportIdMap<ViewportOutput>)
    {
        // viewports not shown anymore -> close their windows
        self.windows.retain(|viewport_id, _| viewport_output.contains_key(viewport_id));
        self.pending_windows.retain(|(viewport_id, _)| viewport_output.contains_key(viewport_id));
        self.failed_windows.retain(|viewport_id| viewport_output.contains_key(viewport_id));
        self.infos.retain(|viewport_id, _| *viewport_id == ViewportId::ROOT || viewport_output.contains_key(viewport_id));

        for (viewport_id, output) in viewport_output
        {
            if output.commands.is_empty()
            {
                continue;
            }

            let window = if *viewport_id == ViewportId::ROOT
            {
                main_window
            }
            else if let Some(viewport) = self.windows.get(viewport_id)
            {
                viewport.window.as_ref()
            }
            else
            {
                continue;
            };

            let mut actions_requested = vec![];
            egui_winit::process_viewport_commands(ctx, self.infos.entry(*viewport_id).or_default(), output.commands.clone(), window, &mut actions_requested);
        }
    }

    fn render_window(&mut self, ctx: &egui::Context, viewport_id: ViewportId, mut output: FullOutput)
    {
        // uploads now (incl. the ones of earlier frames), frees after the main window is rendered (it could still use them this frame)
        self.pending_textures_delta.append(std::mem::take(&mut output.textures_delta));

        for (texture_id, image_deltas) in self.pending_textures_delta.set.drain()
        {
            for image_delta in &image_deltas
            {
                self.renderer.update_texture(&self.device, &self.queue, texture_id, image_delta);
            }
        }

        let Some(viewport) = self.windows.get_mut(&viewport_id) else
        {
            return;
        };

        viewport.ui_state.handle_platform_output(&viewport.window, output.platform_output);

        let size = viewport.window.inner_size();
        if size.width == 0 || size.height == 0 || viewport.window.is_minimized().unwrap_or(false)
        {
            return;
        }

        let surface_texture = match viewport.surface.get_current_texture()
        {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Outdated =>
            {
                viewport.surface.configure(&self.device, &viewport.surface_config);
                return;
            },
            _ => return,
        };

        let screen_descriptor = ScreenDescriptor
        {
            size_in_pixels: [viewport.surface_config.width, viewport.surface_config.height],
            pixels_per_point: output.pixels_per_point,
        };

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("egui viewport") });
        let user_command_buffers = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, &primitives, &screen_descriptor);

        {
            let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());

            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("egui viewport"),
                color_attachments:
                &[
                    Some(wgpu::RenderPassColorAttachment
                    {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations
                        {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // forget_lifetime is intentional -> see EGui::render
            self.renderer.render(&mut pass.forget_lifetime(), &primitives, &screen_descriptor);
        }

        self.queue.submit(user_command_buffers.into_iter().chain(std::iter::once(encoder.finish())));
        self.queue.present(surface_texture);
    }
}

// raw input of a viewport pass - with the infos of all viewports
fn take_viewport_input(infos: &mut ViewportIdMap<ViewportInfo>, viewport_id: ViewportId, ui_state: &mut egui_winit::State, window: &Window) -> RawInput
{
    let mut input = ui_state.take_egui_input(window);
    input.viewports = infos.clone();

    // viewport events (close requests) are delivered once
    if let Some(info) = infos.get_mut(&viewport_id)
    {
        info.events.clear();
    }

    input
}

// egui calls this for every Context::show_viewport_immediate (in the middle of the pass of the parent)
fn install_immediate_viewport_renderer(viewports: &Rc<RefCell<EGuiViewports>>)
{
    let viewports = Rc::downgrade(viewports);

    egui::Context::set_immediate_viewport_renderer(move |ctx, immediate_viewport|
    {
        if let Some(viewports) = viewports.upgrade()
        {
            render_immediate_viewport(ctx, &viewports, immediate_viewport);
        }
    });
}

fn render_immediate_viewport(ctx: &egui::Context, viewports: &Rc<RefCell<EGuiViewports>>, immediate_viewport: ImmediateViewport<'_>)
{
    let ImmediateViewport { ids, builder, mut viewport_ui_cb } = immediate_viewport;

    let input =
    {
        let EGuiViewports { windows, infos, pending_windows, failed_windows, .. } = &mut *viewports.borrow_mut();

        // until the window exists it most likely shows up on the monitor of its parent
        let parent_pixels_per_point = infos.get(&ids.parent).and_then(|info| info.native_pixels_per_point);
        let info = infos.entry(ids.this).or_default();
        info.parent = Some(ids.parent);
        info.native_pixels_per_point = info.native_pixels_per_point.or(parent_pixels_per_point);

        match windows.get_mut(&ids.this)
        {
            Some(viewport) => Some(take_viewport_input(infos, ids.this, &mut viewport.ui_state, &viewport.window)),
            None =>
            {
                if !failed_windows.contains(&ids.this) && !pending_windows.iter().any(|(viewport_id, _)| *viewport_id == ids.this)
                {
                    pending_windows.push((ids.this, builder.clone()));
                }

                None
            }
        }
    };

    match input
    {
        Some(input) =>
        {
            // no borrow while the ui runs (it could show nested viewports)
            let output = ctx.run_ui(input, |ui| viewport_ui_cb(ui));
            viewports.borrow_mut().render_window(ctx, ids.this, output);
        },
        None =>
        {
            // no window yet, but egui expects the ui callback to run -> one pass without painting
            let input = RawInput
            {
                viewport_id: ids.this,
                viewports: viewports.borrow().infos.clone(),
                screen_rect: builder.inner_size.map(|size| egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };

            let mut output = ctx.run_ui(input, |ui| viewport_ui_cb(ui));
            viewports.borrow_mut().pending_textures_delta.append(std::mem::take(&mut output.textures_delta));
        },
    }
}
