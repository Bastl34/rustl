use egui::FullOutput;
use egui_winit::{winit};
use wgpu::{TextureView, CommandEncoder};

use crate::{state::state::State, rendering::wgpu::{WGpu, WGpuRendering}};

pub struct EGui
{
    ctx: egui::Context,
    renderer: egui_wgpu::renderer::Renderer,
    ui_state: egui_winit::State,
    screen_descriptor: egui_wgpu::renderer::ScreenDescriptor,

    output: Option<FullOutput>
}

impl EGui
{
    pub fn new(event_loop: &winit::event_loop::EventLoop<()>, device: &wgpu::Device, surface_cfg: &wgpu::SurfaceConfiguration, window: &winit::window::Window) -> Self
    {
        let size = window.inner_size();

        let mut ui_state = egui_winit::State::new(event_loop);
        ui_state.set_pixels_per_point(window.scale_factor() as f32);

        Self
        {
            ctx: egui::Context::default(),
            renderer: egui_wgpu::renderer::Renderer::new(&device, surface_cfg.format, None, 1),
            ui_state: ui_state,
            screen_descriptor: egui_wgpu::renderer::ScreenDescriptor
            {
                pixels_per_point: window.scale_factor() as f32,
                size_in_pixels: [size.width, size.height],
            },
            output: None
        }
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) -> Vec<egui::ClippedPrimitive>
    {
        let output = self.output.clone().unwrap(); // TODO: check clone

        let clipped_primitives = self.ctx.tessellate(output.shapes);

        self.renderer.update_buffers(device, queue, encoder, &clipped_primitives, &self.screen_descriptor);

        for (tex_id, img_delta) in output.textures_delta.set
        {
            self.renderer.update_texture(&device, &queue, tex_id, &img_delta);
        }

        for tex_id in output.textures_delta.free
        {
            self.renderer.free_texture(&tex_id);
        }

        clipped_primitives
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>, scale_factor: Option<f64>)
    {
        self.screen_descriptor.size_in_pixels[0] = dimensions.width;
        self.screen_descriptor.size_in_pixels[1] = dimensions.height;

        if scale_factor.is_some()
        {
            self.screen_descriptor.pixels_per_point = scale_factor.unwrap() as f32;
            self.ui_state.set_pixels_per_point(scale_factor.unwrap() as f32);
        }
    }

    pub fn on_event(&mut self, event: &winit::event::WindowEvent) -> bool
    {
        let r = self.ui_state.on_event(&self.ctx, event);
        r.consumed
    }

    pub fn request_repaint(&self)
    {
        self.ctx.request_repaint();
    }

    pub fn build(&mut self, state: &mut State, window: &winit::window::Window)
    {
        let raw_input = self.ui_state.take_egui_input(window);

        let full_output = self.ctx.run(raw_input, |ctx|
        {
            egui::Window::new("Settings").show(ctx, |ui|
            {
                ui.label(format!("fps: {}", state.last_fps));
                ui.label("clear color:");
                ui.add(egui::Slider::new(&mut state.clear_color_r, 0.0..=1.0));
                ui.add(egui::Slider::new(&mut state.clear_color_g, 0.0..=1.0));
                ui.add(egui::Slider::new(&mut state.clear_color_b, 0.0..=1.0));

                ui.checkbox(&mut state.fullscreen, "Fullscreen");
            });
        });

        let platform_output = full_output.platform_output.clone();

        self.ui_state.handle_platform_output(window, &self.ctx, platform_output);

        self.output = Some(full_output);
    }
}

impl WGpuRendering for EGui
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        let primitives = self.prepare(wgpu.device(), wgpu.queue_mut(), encoder);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment
                {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations
                    {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    }
                })],
                depth_stencil_attachment: None,
            });

            self.renderer.render(&mut pass, &primitives, &self.screen_descriptor);
        }
    }
}