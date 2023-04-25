use wgpu::{CommandEncoder, TextureView};

use crate::{state::state::{StateItem}, helper::file::{get_current_working_dir, get_current_working_dir_str}};

use super::{wgpu::{WGpuRendering, WGpu}, pipeline::Pipeline, buffer::Buffer};

pub struct Scene
{
    state: StateItem,

    pipe: Pipeline,
    buffer: Buffer,
}

impl Scene
{
    pub fn new(state: StateItem, wgpu: &mut WGpu) -> Scene
    {
        let buffer: Buffer = Buffer::new(wgpu, "test");
        let pipe = Pipeline::new(wgpu, &buffer, "test", "resources/shader/test.wgsl");

        Self
        {
            state,

            pipe,
            buffer
        }
    }
}

impl WGpuRendering for Scene
{
    fn render_pass(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        let state = &*(self.state.borrow());

        let clear_color = wgpu::Color
        {
            a: 1.0,
            r: state.clear_color_r,
            g: state.clear_color_g,
            b: state.clear_color_b,
        };

        let clear_color = wgpu::LoadOp::Clear(clear_color);

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment
            {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations
                {
                    load: clear_color,
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        render_pass.set_pipeline(&self.pipe.get());
        render_pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        render_pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint16); // 1.
        render_pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..1);
    }
}