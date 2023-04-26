use nalgebra::{Vector3, Point3};
use wgpu::{CommandEncoder, TextureView};

use crate::{state::{state::{StateItem}, scene::camera::Camera}, helper::file::{get_current_working_dir, get_current_working_dir_str}};

use super::{wgpu::{WGpuRendering, WGpu}, pipeline::Pipeline, buffer::Buffer, texture::Texture, camera::{CameraUniform}};

pub struct Scene
{
    state: StateItem,

    pipe: Pipeline,
    texture: Texture,
    buffer: Buffer,

    cam: Camera,
    camera_uniform: CameraUniform
}

impl Scene
{
    pub fn new(state: StateItem, wgpu: &mut WGpu) -> Scene
    {
        let buffer = Buffer::new(wgpu, "test");
        let texture = Texture::new(wgpu, "test", "resources/images/test.png");

        let mut cam = Camera::new();
        cam.fovy = 45.0f32.to_radians();
        cam.eye_pos = Point3::<f32>::new(0.0, 1.0, 2.0);
        cam.dir = Vector3::<f32>::new(-cam.eye_pos.x, -cam.eye_pos.y, -cam.eye_pos.z);
        cam.clipping_near = 0.1;
        cam.clipping_far = 100.0;

        cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
        cam.init_matrices();

        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&cam);

        let pipe = Pipeline::new(wgpu, &buffer, "test", "resources/shader/test.wgsl", &texture, &camera_uniform);

        Self
        {
            state,

            texture,
            pipe,
            buffer,

            cam: cam,
            camera_uniform: camera_uniform
        }
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>, _scale_factor: Option<f64>)
    {
        self.cam.init(dimensions.width, dimensions.height);
        self.cam.init_matrices();
    }
}

impl WGpuRendering for Scene
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
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

        self.cam.fovy = state.cam_fov.to_radians();
        self.cam.init_matrices();
        self.camera_uniform.update_view_proj(&self.cam);
        self.pipe.update_camera(wgpu, &self.camera_uniform);

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
        render_pass.set_bind_group(0, &self.pipe.get_diffuse_bind_group(), &[]);
        render_pass.set_bind_group(1, &self.pipe.get_camera_bind_group(), &[]);

        render_pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        render_pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint16); // 1.
        render_pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..1);
    }
}