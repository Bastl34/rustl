#![allow(dead_code)]

use std::sync::Arc;

use egui::FullOutput;
use egui_wgpu::{Renderer, ScreenDescriptor};
use egui_winit::winit;
use wgpu::{TextureView, CommandEncoder};

use crate::rendering::{gpu_timer::{GpuTimer, GpuTimerPass}, wgpu::WGpu};

pub struct EGui
{
    pub ctx: egui::Context,
    pub renderer: egui_wgpu::Renderer,
    pub ui_state: egui_winit::State,
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,

    pub output: Option<FullOutput>,
    pub pending_textures_delta: egui::TexturesDelta,

    // gpu time of the egui render pass (None if the adapter does not support timestamp queries)
    gpu_timer: Option<GpuTimer>,
}

impl EGui
{
    pub fn new(device: &wgpu::Device, surface_cfg: &wgpu::SurfaceConfiguration, window: Arc<winit::window::Window>) -> Self
    {
        let size = window.inner_size();

        let ctx: egui::Context = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let viewport_id = ctx.viewport_id();

        let native_pixels_per_point = window.scale_factor() as f32;
        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let theme = Some(winit::window::Theme::Dark);
        let ui_state = egui_winit::State::new(ctx.clone(), viewport_id, &window, Some(native_pixels_per_point), theme, Some(max_texture_side));

        Self
        {
            ctx: ctx,
            renderer: Renderer::new(&device, surface_cfg.format, egui_wgpu::RendererOptions
            {
                dithering: true,
                ..Default::default()
            }),
            ui_state: ui_state,
            screen_descriptor: ScreenDescriptor
            {
                pixels_per_point: window.scale_factor() as f32,
                size_in_pixels: [size.width, size.height],
            },
            output: None,
            pending_textures_delta: egui::TexturesDelta::default(),

            gpu_timer: GpuTimer::new(device),
        }
    }

    // accumulates the texture delta of the current frame so it survives a skipped render
    // (e.g. when wgpu.start_render() returns None during resize/surface reconfigure)
    pub fn set_output(&mut self, mut output: FullOutput)
    {
        let delta = std::mem::take(&mut output.textures_delta);
        self.pending_textures_delta.append(delta);
        self.output = Some(output);
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) -> Vec<egui::ClippedPrimitive>
    {
        let output = self.output.clone().unwrap();
        let clipped_primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        self.renderer.update_buffers(device, queue, encoder, &clipped_primitives, &self.screen_descriptor);

        let mut textures_delta = std::mem::take(&mut self.pending_textures_delta);

        for (tex_id, img_deltas) in &textures_delta.set
        {
            for img_delta in img_deltas
            {
                self.renderer.update_texture(&device, &queue, *tex_id, img_delta);
            }
        }

        for tex_id in &textures_delta.free
        {
            self.renderer.free_texture(tex_id);
        }

        textures_delta.clear();

        clipped_primitives
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
            self.renderer.render(&mut pass.forget_lifetime(), &primitives, &self.screen_descriptor);
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