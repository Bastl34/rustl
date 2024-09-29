use std::sync::Arc;

use egui::FullOutput;
use egui_wgpu::{Renderer, ScreenDescriptor};
use egui_winit::winit;
use wgpu::{TextureView, CommandEncoder};

use crate::rendering::wgpu::WGpu;

pub struct EGui
{
    pub ctx: egui::Context,
    pub renderer: egui_wgpu::Renderer,
    pub ui_state: egui_winit::State,
    pub screen_descriptor: egui_wgpu::ScreenDescriptor,

    pub output: Option<FullOutput>
}

impl EGui
{
    pub fn new(device: &wgpu::Device, surface_cfg: &wgpu::SurfaceConfiguration, window: Arc<winit::window::Window>) -> Self
    {
        let size = window.inner_size();

        let ctx: egui::Context = egui::Context::default();
        let viewport_id = ctx.viewport_id();

        let native_pixels_per_point = window.scale_factor() as f32;
        let max_texture_side = device.limits().max_texture_dimension_2d as usize;
        let theme = Some(winit::window::Theme::Dark);
        let ui_state = egui_winit::State::new(ctx.clone(), viewport_id, &window, Some(native_pixels_per_point), theme, Some(max_texture_side));

        let dithering = true;

        Self
        {
            ctx: ctx,
            renderer: Renderer::new(&device, surface_cfg.format, None, 1, dithering),
            ui_state: ui_state,
            screen_descriptor: ScreenDescriptor
            {
                pixels_per_point: window.scale_factor() as f32,
                size_in_pixels: [size.width, size.height],
            },
            output: None
        }
    }

    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) -> Vec<egui::ClippedPrimitive>
    {
        let output = self.output.clone().unwrap();
        let clipped_primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

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
        let primitives = self.prepare(wgpu.device(), wgpu.queue_mut(), encoder);

        {
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
                        }
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // forget_lifetime is intentional -> see render description
            // https://github.com/emilk/egui/pull/5149
            self.renderer.render(&mut pass.forget_lifetime(), &primitives, &self.screen_descriptor);
        }
    }
}