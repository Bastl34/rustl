use std::{borrow::Cow, sync::RwLockReadGuard};

use nalgebra::{Vector3};
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment};

use crate::{state::{state::{State}, scene::{instance::{Instance, self}, components::{material::Material, component::Component}, node::{Node, NodeItem}}, helper::render_item::{get_render_item, RenderItemType, get_render_item_mut, RenderItem}}, helper::image::float32_to_grayscale, resources::resources, shared_component_write, render_item_impl_default};

use super::{wgpu::{WGpu}, pipeline::Pipeline, texture::Texture, camera::{CameraUniform}, instance::{InstanceBuffer}, vertex_buffer::VertexBuffer, light::LightUniform};

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

    //instance_amount: u32,
    //instance_buffer: wgpu::Buffer,

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
            vertex_buffer = VertexBuffer::new(wgpu, "vertex buffer", *mesh);

            mesh.get_base_mut().render_item = Some(Box::new(vertex_buffer));
        }

        // instance
        {
            let instance_buffer;
            {
                let node_read = node.read().unwrap();

                instance_buffer = InstanceBuffer::new(wgpu, "instance buffer", &node_read.instances);
            }

            let mut node = node.write().unwrap();
            node.instance_render_item = Some(Box::new(instance_buffer));
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

        let render_scene = Self
        {
            clear_color: wgpu::Color::BLACK,

            //base_texture,
            //normal_texture,

            color_pipe,
            depth_pipe,

            depth_buffer_texture,
            depth_pass_buffer_texture,
            //buffer,

            //instance_amount: instance_amount as u32,
            //instance_buffer,

            //camera_uniform: camera_uniform,
            //light_uniform: light_uniform
        };

        //render_scene.prepare(wgpu, scene);

        render_scene
    }

    /*
    pub fn prepare(&mut self, wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>)
    {
    }
    */

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene)
    {
        self.clear_color = wgpu::Color
        {
            a: 1.0,
            r: state.clear_color.x as f64,
            g: state.clear_color.y as f64,
            b: state.clear_color.z as f64,
        };

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

            let instance_buffer;
            {
                let mut node = node_arc.read().unwrap();

                instance_buffer = InstanceBuffer::new(wgpu, "instance buffer", &node.instances);
            }

            node_arc.write().unwrap().instance_render_item = Some(Box::new(instance_buffer));
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

    fn list_all_child_nodes(nodes: &Vec<NodeItem>) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            let child_nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes);

            if node.read().unwrap().render_children_first {
                all_nodes.extend(child_nodes);
                all_nodes.push(node.clone());
            } else {
                all_nodes.push(node.clone());
                all_nodes.extend(child_nodes);
            }
        }

        all_nodes
    }

    pub fn render(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, scene: &Box<crate::state::scene::scene::Scene>) -> u32
    {
        let all_nodes = Scene::list_all_child_nodes(&scene.nodes);
        let mut read_nodes = vec![];

        for node in &all_nodes
        {
            read_nodes.push(node.read().unwrap());
        }

        let mut draw_calls: u32 = 0;
        draw_calls += self.render_depth(wgpu, view, encoder, &read_nodes);
        draw_calls += self.render_color(wgpu, view, encoder, &read_nodes);

        draw_calls
    }

    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>) -> u32
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

        self.draw_phase(&mut render_pass, &self.depth_pipe, nodes)
    }

    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>) -> u32
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

        self.draw_phase(&mut render_pass, &self.color_pipe, nodes)
    }

    fn draw_phase<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a Pipeline, nodes: &'a Vec<RwLockReadGuard<Box<Node>>>) -> u32
    {
        let mut draw_calls: u32 = 0;

        for node in nodes
        {
            let mesh = node.get_mesh();

            if mesh.is_none()
            {
                continue;
            }

            let mesh = mesh.unwrap();

            if let Some(render_item) = mesh.get_base().render_item.as_ref()
            {
                let vertex_buffer = get_render_item::<VertexBuffer>(&render_item);

                let instance_render_item = node.instance_render_item.as_ref().unwrap();
                let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);

                pass.set_pipeline(&pipeline.get());
                pass.set_bind_group(0, pipeline.get_textures_bind_group(), &[]);
                pass.set_bind_group(1, pipeline.get_camera_bind_group(), &[]);
                pass.set_bind_group(2, pipeline.get_light_bind_group(), &[]);

                pass.set_vertex_buffer(0, vertex_buffer.get_vertex_buffer().slice(..));

                // instancing
                pass.set_vertex_buffer(1, instance_buffer.get_buffer().slice(..));

                pass.set_index_buffer(vertex_buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..vertex_buffer.get_index_count(), 0, 0..instance_buffer.get_count() as _);

                draw_calls += 1;
            }
        }

        draw_calls
    }

}