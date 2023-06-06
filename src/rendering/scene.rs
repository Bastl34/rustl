use nalgebra::{Vector3};
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment};

use crate::{state::{state::{State}, scene::{instance::{Instance, self}, components::{material::Material, component::Component}}, helper::render_item::{get_render_item, RenderItemType, get_render_item_mut, RenderItem}}, helper::image::float32_to_grayscale, resources::resources, shared_component_write, render_item_impl_default};

use super::{wgpu::{WGpu}, pipeline::Pipeline, texture::Texture, camera::{CameraUniform}, instance::instances_to_buffer, vertex_buffer::VertexBuffer, light::LightUniform};

type MaterialComponent = crate::state::scene::components::material::Material;
type MeshComponent = crate::state::scene::components::mesh::Mesh;

pub struct Scene
{
    clear_color: wgpu::Color,

    depth_pipe: Pipeline,
    color_pipe: Pipeline,

    depth_pass_buffer_texture: Texture,
    depth_buffer_texture: Texture,

    //base_texture: Texture,
    //normal_texture: Texture,

    //buffer: VertexBuffer,

    instance_amount: u32,
    instance_buffer: wgpu::Buffer,

    //camera_uniform: CameraUniform,
    //light_uniform: LightUniform,
}

impl RenderItem for Scene
{
    render_item_impl_default!();
}

impl Scene
{
    pub async fn new(wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>) -> Scene
    {
        let node_id = 0;
        let node = scene.nodes.get_mut(node_id).unwrap();

        /*
        {
            let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>().unwrap();
            dbg!(&mesh.normals);
        }

        {
            let mesh = node.find_component_mut::<crate::state::scene::components::mesh::Mesh>().unwrap();
            mesh.normals.clear();
            dbg!(&mesh.normals);
        }
        */

        /*
        just as reference
        {

            let mat = node.read().unwrap().find_shared_component::<MaterialComponent>().unwrap();
            let mat = mat.read().unwrap().as_any().downcast_ref::<MaterialComponent>().unwrap();
        }

        {
            let mat = node.write().unwrap().find_shared_component_mut::<MaterialComponent>().unwrap();
            shared_component_write!(mat, MaterialComponent, mat);
            let mat_data = mat.get_data_mut();

            mat_data.alpha = 1.0;
        }
        */

        // vertex buffer
        let vertex_buffer;
        {
            let mut node = node.write().unwrap();
            let mesh = node.find_component_mut::<crate::state::scene::components::mesh::Mesh>().unwrap();
            vertex_buffer = VertexBuffer::new(wgpu, "test", *mesh);

            mesh.get_base_mut().render_item = Some(Box::new(vertex_buffer));
        }

        // camera
        let mut camera_uniform;
        {
            let cam_id = 0; // TODO
            let mut cam = scene.cameras.get_mut(cam_id).unwrap();

            camera_uniform = CameraUniform::new();
            camera_uniform.update_view_proj(cam.eye_pos, cam.webgpu_projection(), cam.view);

            cam.render_item = Some(Box::new(camera_uniform));
        }

        // light
        let light_uniform;
        {
            let light_id = 0; // TODO
            let mut light = &mut scene.lights.get_mut(light_id).unwrap();

            light_uniform = LightUniform::new(light.pos, light.color, light.intensity);
            light.render_item = Some(Box::new(light_uniform));

            //let mut light_uniform = LightUniform::new(light.pos, light.color, light.intensity);
        }

        {
            let instance = Instance::new_with_data
            (
                scene.id_manager.get_next_instance_id(),
                "instance".to_string(),
                node.clone(),
                Vector3::<f32>::new(0.0, 0.0, 0.0),
                Vector3::<f32>::new(0.0, 2.0, 0.0),
                Vector3::<f32>::new(1.0, 1.0, 1.0)
            );

            node.write().unwrap().add_instance(Box::new(instance));
        }

        let instance_buffer;
        let instance_amount;
        {
            let node = node.read().unwrap();
            instance_buffer = instances_to_buffer(wgpu, &node.instances);
            instance_amount = node.instances.len();
        }

        let color_pipe;
        let depth_pipe;
        let depth_buffer_texture;
        let depth_pass_buffer_texture;
        {
            let mat = node.write().unwrap().find_shared_component::<MaterialComponent>().unwrap();
            let mut mat = mat.write().unwrap();
            let mat = mat.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();
            let mat_data = mat.get_data_mut();

            let mut base_tex = mat_data.texture_base.as_mut().unwrap().write().unwrap();
            let mut normal_tex = mat_data.texture_normal.as_mut().unwrap().write().unwrap();

            let base_texture = Texture::new_from_texture(wgpu, base_tex.name.as_str(), &base_tex, true);
            let normal_texture = Texture::new_from_texture(wgpu, normal_tex.name.as_str(), &normal_tex, false);



            depth_buffer_texture = Texture::new_depth_texture(wgpu);
            depth_pass_buffer_texture = Texture::new_depth_texture(wgpu);

            let depth_buffer_texture = Texture::new_depth_texture(wgpu);

             // ********** depth pass **********
             let mut textures = vec![];
             textures.push(&base_texture);
             textures.push(&normal_texture);

             let shader_source = resources::load_string_async("shader/depth.wgsl").await.unwrap();
             depth_pipe = Pipeline::new(wgpu, "test", &shader_source, &textures, &camera_uniform, &light_uniform, true, true);

             // ********** color pass **********
            //let mut textures = vec![];
            textures.push(&depth_pass_buffer_texture);

            let shader_source = resources::load_string_async("shader/phong.wgsl").await.unwrap();
            color_pipe = Pipeline::new(wgpu, "test", &shader_source, &textures, &camera_uniform, &light_uniform, true, true);


            base_tex.render_item = Some(Box::new(base_texture));
            normal_tex.render_item = Some(Box::new(normal_texture));
        }

        Self
        {
            clear_color: wgpu::Color::BLACK,

            //base_texture,
            //normal_texture,

            color_pipe,
            depth_pipe,

            depth_buffer_texture,
            depth_pass_buffer_texture,
            //buffer,

            instance_amount: instance_amount as u32,
            instance_buffer,

            //camera_uniform: camera_uniform,
            //light_uniform: light_uniform
        }
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene_id: usize)
    {
        self.clear_color = wgpu::Color
        {
            a: 1.0,
            r: state.clear_color.x as f64,
            g: state.clear_color.y as f64,
            b: state.clear_color.z as f64,
        };

        let scene = state.scenes.get_mut(scene_id).unwrap();

        for cam in &mut scene.cameras
        {
            cam.eye_pos = state.camera_pos;
            cam.fovy = state.cam_fov.to_radians();
            cam.init_matrices();

            let projection = cam.webgpu_projection().clone();
            let view = cam.view.clone();

            let render_item = get_render_item_mut::<CameraUniform>(cam.render_item.as_mut().unwrap());
            render_item.update_view_proj(cam.eye_pos, projection, view);

            self.color_pipe.update_camera(wgpu, *render_item);
            self.depth_pipe.update_camera(wgpu, *render_item);
        }

        let node_id = 0;

        {
            let node_arc = scene.nodes.get_mut(node_id).unwrap();
            let mut node = node_arc.write().unwrap();

            {
                let instances = &mut node.instances;

                if instances.len() != state.instances as usize
                {
                    instances.clear();

                    for i in 0..state.instances
                    {
                        let x = (i as f32 * 5.0) - ((state.instances - 1) as f32 * 5.0) / 2.0;

                        let instance = Instance::new_with_data
                        (
                            scene.id_manager.get_next_instance_id(),
                            "instance".to_string(),
                            node_arc.clone(),
                            Vector3::<f32>::new(x, 0.0, 0.0),
                            Vector3::<f32>::new(0.0, i as f32, 0.0),
                            Vector3::<f32>::new(1.0, 1.0, 1.0)
                        );

                        node.add_instance(Box::new(instance));
                    }
                }
                else
                {
                    for instance in instances
                    {
                        let rotation = state.rotation_speed * state.frame_scale;
                        instance.apply_rotation(Vector3::<f32>::new(0.0, rotation, 0.0));
                    }
                }
            }
        }
        {
            let node_arc = scene.nodes.get_mut(node_id).unwrap();
            let node = node_arc.read().unwrap();

            if node.instances.len() > 0
            {
                self.instance_buffer = instances_to_buffer(wgpu, &node.instances);
            }

            self.instance_amount = node.instances.len() as u32;
        }

        {
            /*
            let node_arc = scene.nodes.get_mut(node_id).unwrap();
            let mut node_write = node_arc.write().unwrap();

            let transform = node_write.find_component_mut::<Transformation>();
            */
            //transform.unwrap().calc_full_transform(node_arc.clone());
            //transform.unwrap().calc_full_transform(node_write.as_mut());
            //transform.unwrap().calc_full_transform(node_arc.clone());

            let node_arc = scene.nodes.get_mut(node_id).unwrap();
        }

        // light
        if scene.lights.len() > 0
        {
            let mut light = scene.lights.get_mut(0).unwrap();
            light.color = state.light_color.clone();
            light.pos = state.light_pos.clone();

            let render_item = get_render_item_mut::<LightUniform>(light.render_item.as_mut().unwrap());

            render_item.color = [light.color.x, light.color.y, light.color.z, 1.0];
            render_item.position =  [light.pos.x, light.pos.y, light.pos.z, 1.0];
            self.color_pipe.update_light(wgpu, *render_item);
        }

        if state.save_image
        {
            let node_arc = scene.nodes.get(node_id).unwrap();
            //let node = node_arc.read().unwrap();

            let mat = node_arc.read().unwrap().find_shared_component::<MaterialComponent>().unwrap();
            let mat = mat.read().unwrap();
            let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();

            let data = mat.get_data();

            {
                let base_tex = data.texture_base.clone().unwrap();
                let base_tex = base_tex.read().unwrap();
                let render_item = base_tex.render_item.as_ref().unwrap();
                let render_item = get_render_item::<Texture>(&render_item);

                let img_data = render_item.to_image(wgpu);
                img_data.save("data/base_texture.png").unwrap();
            }

            {
                let base_tex = data.texture_normal.clone().unwrap();
                let base_tex = base_tex.read().unwrap();
                let render_item = base_tex.render_item.as_ref().unwrap();
                let render_item = get_render_item::<Texture>(&render_item);

                let img_data = render_item.to_image(wgpu);
                img_data.save("data/normal_texture.png").unwrap();
            }

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

    pub fn resize(&mut self, wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>)
    {
        for cam in &mut scene.cameras
        {
            cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.init_matrices();
        }

        self.depth_buffer_texture = Texture::new_depth_texture(wgpu);
        self.depth_pass_buffer_texture = Texture::new_depth_texture(wgpu);
    }

    pub fn render(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        self.render_depth(wgpu, view, encoder);
        self.render_color(wgpu, view, encoder);
    }

    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
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

        self.draw_phase(&mut render_pass, &self.depth_pipe);
    }

    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
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

        self.draw_phase(&mut render_pass, &self.color_pipe);
    }

    fn draw_phase<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a Pipeline)
    {
        /*
        pass.set_pipeline(&pipeline.get());
        pass.set_bind_group(0, pipeline.get_textures_bind_group(), &[]);
        pass.set_bind_group(1, pipeline.get_camera_bind_group(), &[]);
        pass.set_bind_group(2, pipeline.get_light_bind_group(), &[]);

        pass.set_vertex_buffer(0, self.buffer.get_vertex_buffer().slice(..));

        // instancing
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        pass.set_index_buffer(self.buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.buffer.get_index_count(), 0, 0..self.instance_amount as _);
        */
    }

}

/*
impl WGpuRendering for Scene
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder)
    {
        self.render_depth(wgpu, view, encoder);
        self.render_color(wgpu, view, encoder);
    }
}
*/