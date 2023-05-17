use log::info;
use nalgebra::{Vector3, Point3};
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment};

use crate::{state::{state::{State}, scene::{camera::Camera, instance::Instance}}, helper::image::float32_to_grayscale, resources::resources};

use super::{wgpu::{WGpuRendering, WGpu}, pipeline::Pipeline, texture::Texture, camera::{CameraUniform}, instance::instances_to_buffer, vertex_buffer::VertexBuffer};

pub struct Scene
{
    clear_color: wgpu::Color,

    depth_pipe: Pipeline,
    color_pipe: Pipeline,

    depth_pass_buffer_texture: Texture,
    depth_buffer_texture: Texture,
    texture: Texture,
    buffer: VertexBuffer,

    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,

    cam: Camera,
    camera_uniform: CameraUniform
}

impl Scene
{
    pub async fn new(wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>) -> Scene
    {
        let node = scene.nodes.get_mut(0).unwrap();

        {
            let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>().unwrap();
            dbg!(&mesh.normals);
        }

        {
            let mesh = node.find_component_mut::<crate::state::scene::components::mesh::Mesh>().unwrap();
            mesh.normals.clear();
            dbg!(&mesh.normals);
        }

        let buffer = VertexBuffer::new(wgpu, "test");

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

        let tex_image = resources::load_binary_async("images/test_2.png").await.unwrap();
        let texture = Texture::new_from_image(wgpu, "test", &tex_image);

        let depth_buffer_texture = Texture::new_depth_texture(wgpu);
        let depth_pass_buffer_texture = Texture::new_depth_texture(wgpu);

         // ********** depth pass **********
         let mut textures = vec![];
         textures.push(&texture);

         let shader_source = resources::load_string_async("shader/depth.wgsl").await.unwrap();
         let depth_pipe = Pipeline::new(wgpu, &buffer, "test", &shader_source, &textures, &camera_uniform, true, true);

         // ********** color pass **********
        //let mut textures = vec![];
        textures.push(&depth_pass_buffer_texture);

        let shader_source = resources::load_string_async("shader/test.wgsl").await.unwrap();
        let color_pipe = Pipeline::new(wgpu, &buffer, "test", &shader_source, &textures, &camera_uniform, true, true);

        Self
        {
            clear_color: wgpu::Color::BLACK,

            texture,

            color_pipe,
            depth_pipe,

            depth_buffer_texture,
            depth_pass_buffer_texture,
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

        self.color_pipe.update_camera(wgpu, &self.camera_uniform);
        self.depth_pipe.update_camera(wgpu, &self.camera_uniform);

        self.instances.clear();

        for i in 0..state.instances
        {
            let x = (-((state.instances as f32) / 2.0) * 0.5) + (i as f32 * 0.5);
            self.instances.push(Instance::new(Vector3::<f32>::new(x, 0.0, 0.0), Vector3::<f32>::new(0.0, i as f32, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0)));
        }

        if self.instances.len() > 0
        {
            self.instance_buffer = instances_to_buffer(wgpu, &self.instances);
        }

        if state.save_image
        {
            let img_data = self.texture.to_image(wgpu);
            img_data.save("data/texture.png").unwrap();
            state.save_image = false;
        }

        if state.save_depth_pass_image
        {
            let img_data = self.depth_pass_buffer_texture.to_image(wgpu);
            img_data.save("data/depth_pass.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_pass_gray.png").unwrap();

            state.save_depth_pass_image = false;
        }

        if state.save_depth_buffer_image
        {
            let img_data = self.depth_buffer_texture.to_image(wgpu);
            img_data.save("data/depth_buffer.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_buffer_gray.png").unwrap();

            state.save_depth_buffer_image = false;
        }

    }

    pub fn resize(&mut self, wgpu: &mut WGpu)
    {
        self.cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
        self.cam.init_matrices();

        self.depth_buffer_texture = Texture::new_depth_texture(wgpu);
        self.depth_pass_buffer_texture = Texture::new_depth_texture(wgpu);
    }

    fn render_depth(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        let clear_color = wgpu::Color::BLACK;

        let mut color_attachments: &[Option<RenderPassColorAttachment>] = &[
            Some(wgpu::RenderPassColorAttachment
            {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: true,
                },
            })
        ];

        if !self.depth_pipe.fragment_attachment
        {
            color_attachments = &[];
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("depth pass"),
            color_attachments: color_attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_pass_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        render_pass.set_pipeline(&self.depth_pipe.get());
        render_pass.set_bind_group(0, &self.depth_pipe.get_textures_bind_group(), &[]);
        render_pass.set_bind_group(1, &self.depth_pipe.get_camera_bind_group(), &[]);

        render_pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        // instancing
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        render_pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..self.instances.len() as _);
    }

    fn render_color(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("color pass"),
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
                view: &self.depth_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        render_pass.set_pipeline(&self.color_pipe.get());
        render_pass.set_bind_group(0, &self.color_pipe.get_textures_bind_group(), &[]);
        render_pass.set_bind_group(1, &self.color_pipe.get_camera_bind_group(), &[]);

        render_pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        // instancing
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        render_pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..self.instances.len() as _);
    }
}

impl WGpuRendering for Scene
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        self.render_depth(wgpu, view, encoder);
        self.render_color(wgpu, view, encoder);
    }
}