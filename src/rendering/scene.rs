use nalgebra::{Vector3, Point3};
use wgpu::{CommandEncoder, TextureView};

use crate::{state::{state::{State}, scene::{camera::Camera, instance::Instance}}};

use super::{wgpu::{WGpuRendering, WGpu}, pipeline::Pipeline, buffer::Buffer, texture::Texture, camera::{CameraUniform}, instance::instances_to_buffer};

pub struct Scene
{
    clear_color: wgpu::Color,

    pipe: Pipeline,
    depth_texture: Texture,
    texture: Texture,
    buffer: Buffer,

    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,

    cam: Camera,
    camera_uniform: CameraUniform
}

impl Scene
{
    pub fn new(wgpu: &mut WGpu) -> Scene
    {
        let buffer = Buffer::new(wgpu, "test");

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

        let mut instances = vec![];
        instances.push(Instance::new(Vector3::<f32>::new(0.0, 0.0, 0.0), Vector3::<f32>::new(0.0, 0.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0)));
        instances.push(Instance::new(Vector3::<f32>::new(-1.0, 0.0, 0.0), Vector3::<f32>::new(0.0, 40.0f32.to_radians(), 0.0), Vector3::<f32>::new(0.8, 0.8, 0.8)));
        instances.push(Instance::new(Vector3::<f32>::new(1.0, 0.0, 0.0), Vector3::<f32>::new(0.0, -40.0f32.to_radians(), 0.0), Vector3::<f32>::new(1.2, 1.2, 1.2)));

        let instance_buffer = instances_to_buffer(wgpu, &instances);

        let texture = Texture::new_from_image(wgpu, "test", "resources/images/test_2.png");
        let depth_texture = Texture::new_depth_texture(wgpu);

        let mut textures = vec![];
        textures.push(&texture);
        //textures.push(&depth_texture);

        let pipe = Pipeline::new(wgpu, &buffer, "test", "resources/shader/test.wgsl", &textures, &camera_uniform, true);

        Self
        {
            clear_color: wgpu::Color::BLACK,

            texture,
            pipe,
            depth_texture,
            buffer,
            instances,
            instance_buffer,

            cam: cam,
            camera_uniform: camera_uniform
        }
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State)
    {
        self.clear_color = wgpu::Color
        {
            a: 1.0,
            r: state.clear_color_r,
            g: state.clear_color_g,
            b: state.clear_color_b,
        };

        self.cam.fovy = state.cam_fov.to_radians();
        self.cam.init_matrices();
        self.camera_uniform.update_view_proj(&self.cam);
        self.pipe.update_camera(wgpu, &self.camera_uniform);

        self.instances.clear();

        for i in 0..state.instances
        {
            let x = (-(state.instances as f32) / 2.0) + (i as f32 * 0.5);
            self.instances.push(Instance::new(Vector3::<f32>::new(x, 0.0, 0.0), Vector3::<f32>::new(0.0, i as f32, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0)));
        }

        if self.instances.len() > 0
        {
            self.instance_buffer = instances_to_buffer(wgpu, &self.instances);
        }

        if state.save_image
        {
            let img_data = self.texture.to_image(wgpu);
            img_data.save("data/texture.png");
            state.save_image = false;
        }

        if state.save_depth_image
        {
            let img_data = self.depth_texture.to_image(wgpu);
            img_data.save("data/depth.png");
            state.save_depth_image = false;
        }

    }

    pub fn resize(&mut self, wgpu: &mut WGpu)
    {
        self.cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
        self.cam.init_matrices();

        self.depth_texture = Texture::new_depth_texture(wgpu);
    }
}

impl WGpuRendering for Scene
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
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
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: true,
                    },
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        render_pass.set_pipeline(&self.pipe.get());
        render_pass.set_bind_group(0, &self.pipe.get_textures_bind_group(), &[]);
        render_pass.set_bind_group(1, &self.pipe.get_camera_bind_group(), &[]);

        render_pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        // instancing
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        render_pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..self.instances.len() as _);
    }
}