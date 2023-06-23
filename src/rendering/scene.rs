use std::{sync::RwLockReadGuard};

use nalgebra::{Vector3};
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment};

use crate::{state::{state::{State}, scene::{instance::{Instance}, components::{component::Component}, node::{Node, NodeItem}}, helper::render_item::{get_render_item, get_render_item_mut, RenderItem}}, helper::image::float32_to_grayscale, resources::resources, render_item_impl_default};

use super::{wgpu::{WGpu}, pipeline::Pipeline, texture::Texture, camera::{CameraBuffer}, instance::{InstanceBuffer}, vertex_buffer::VertexBuffer, light::{LightBuffer}};

type MaterialComponent = crate::state::scene::components::material::Material;
//type MeshComponent = crate::state::scene::components::mesh::Mesh;

pub struct Scene
{
    clear_color: wgpu::Color,

    color_shader: String,
    depth_shader: String,

    samples: u32,


    depth_pipe: Option<Pipeline>,
    color_pipe: Option<Pipeline>,

    depth_pass_buffer_texture: Option<Texture>,
    depth_buffer_texture: Option<Texture>,
}

impl RenderItem for Scene
{
    render_item_impl_default!();
}

impl Scene
{
    pub async fn new(wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>, samples: u32) -> Scene
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

                instance_buffer = InstanceBuffer::new(wgpu, "instance buffer", node_read.instances.get_ref());
            }

            let mut node = node.write().unwrap();
            node.instance_render_item = Some(Box::new(instance_buffer));
        }

        // camera
        let cam_id = 0; // TODO
        {
            let mut cam = scene.cameras.get_mut(cam_id).unwrap();

            let camera_buffer = CameraBuffer::new(wgpu, &cam);
            cam.render_item = Some(Box::new(camera_buffer));
        }

        // lights
        {
            let lights_buffer = LightBuffer::new(wgpu, "lights buffer".to_string(), &scene.lights);
            scene.lights_render_item = Some(Box::new(lights_buffer));
        }
        /*
        for light in scene.lights.iter_mut()
        {
            let light_buffer = LightBuffer::new(wgpu, &light);
            light.render_item = Some(Box::new(light_buffer));
        }
        */

        // shader source
        let color_shader = resources::load_string_async("shader/phong.wgsl").await.unwrap();
        let depth_shader = resources::load_string_async("shader/depth.wgsl").await.unwrap();

        let mut render_scene = Self
        {
            clear_color: wgpu::Color::BLACK,

            color_shader,
            depth_shader,

            samples,

            color_pipe: None,
            depth_pipe: None,

            depth_buffer_texture: None,
            depth_pass_buffer_texture: None,
        };

        render_scene.create_pipelines(wgpu, scene, false);

        render_scene
    }

    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, re_create: bool)
    {
        let node_id = 0;
        let cam_id = 0;

        let node = scene.nodes.get_mut(node_id).unwrap();

        // material and textures
        let mat = node.write().unwrap().find_shared_component::<MaterialComponent>().unwrap();
        let mut mat = mat.write().unwrap();
        let mat = mat.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();
        let mat_data = mat.get_data_mut();

        let mut base_tex = mat_data.texture_base.as_mut().unwrap().write().unwrap();
        let mut normal_tex = mat_data.texture_normal.as_mut().unwrap().write().unwrap();

        let base_texture = Texture::new_from_texture(wgpu, base_tex.name.as_str(), &base_tex, true);
        let normal_texture = Texture::new_from_texture(wgpu, normal_tex.name.as_str(), &normal_tex, false);

        let depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples);
        let depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1);

        //light
        let lights_render_item = get_render_item::<LightBuffer>(scene.lights_render_item.as_ref().unwrap());

        /*
        let mut lights: Vec<Box<&LightBuffer>> = vec![];
        for light in &scene.lights
        {
            let render_item = get_render_item::<LightBuffer>(light.render_item.as_ref().unwrap());
            lights.push(render_item);
        }
        */

        // cam
        let cam = scene.cameras.get(cam_id).unwrap();
        let cam_render_item = get_render_item::<CameraBuffer>(cam.render_item.as_ref().unwrap());

        // ********** depth pass **********
        let mut textures = vec![];
        textures.push(&base_texture);
        textures.push(&normal_texture);

        if !re_create
        {
            self.depth_pipe = Some(Pipeline::new(wgpu, "depth pipe", &self.depth_shader, &textures, &cam_render_item, &lights_render_item, true, true, 1));
        }
        else
        {
            self.depth_pipe.as_mut().unwrap().re_create(wgpu, &textures, &cam_render_item, &lights_render_item, true, true, 1);
        }

        // ********** color pass **********
        //let mut textures = vec![];
        textures.push(&depth_pass_buffer_texture);

        if !re_create
        {
            self.color_pipe = Some(Pipeline::new(wgpu, "color pipe", &self.color_shader, &textures, &cam_render_item, &lights_render_item, true, true, self.samples));
        }
        else
        {
            self.color_pipe.as_mut().unwrap().re_create(wgpu, &textures, &cam_render_item, &lights_render_item, true, true, self.samples);
        }

        base_tex.render_item = Some(Box::new(base_texture));
        normal_tex.render_item = Some(Box::new(normal_texture));

        self.depth_buffer_texture = Some(depth_buffer_texture);
        self.depth_pass_buffer_texture = Some(depth_pass_buffer_texture);
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene)
    {
        let node_id = 0;

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

            let mut render_item = cam.render_item.take();

            {
                let render_item = get_render_item_mut::<CameraBuffer>(render_item.as_mut().unwrap());
                render_item.update_buffer(wgpu, cam.as_ref());
            }

            cam.render_item = render_item;
        }


        {
            let node_arc = scene.nodes.get_mut(node_id).unwrap();
            let mut node = node_arc.write().unwrap();

            {
                let instances = &mut node.instances;

                if instances.get_ref().len() != state.instances as usize
                {
                    dbg!("recreate instances");

                    instances.get_mut().clear();

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
                    for instance in instances.get_mut()
                    {
                        let rotation: f32 = state.rotation_speed * state.frame_scale;
                        instance.apply_rotation(Vector3::<f32>::new(0.0, rotation, 0.0));
                    }
                }
            }
        }

        {
            let node_arc = scene.nodes.get_mut(node_id).unwrap();

            let changed;
            {
                let mut write = node_arc.write().unwrap();
                (_, changed) = write.instances.consume_borrow();
            }

            if changed
            {
                let instance_buffer;
                {
                    let node = node_arc.read().unwrap();
                    instance_buffer = InstanceBuffer::new(wgpu, "instance buffer", node.instances.get_ref());
                }

                node_arc.write().unwrap().instance_render_item = Some(Box::new(instance_buffer));
            }
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
            {
                let light_id = 0;

                let mut light = scene.lights.get_mut(light_id).unwrap();
                light.color = state.light1_color.clone();
                light.pos = state.light1_pos.clone();

                let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
                render_item.update_buffer(wgpu, light, light_id);

                /*

                let mut render_item = light.render_item.take();

                {
                    let render_item = get_render_item_mut::<LightBuffer>(render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, light.as_ref());
                }

                light.render_item = render_item;
                */
            }

            {
                let light_id = 1;

                let mut light = scene.lights.get_mut(light_id).unwrap();
                light.color = state.light2_color.clone();
                light.pos = state.light2_pos.clone();

                let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
                render_item.update_buffer(wgpu, light, light_id);

                /*
                let mut render_item = light.render_item.take();

                {
                    let render_item = get_render_item_mut::<LightBuffer>(render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, light.as_ref());
                }

                light.render_item = render_item;
                */
            }
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
            let img_data = self.depth_pass_buffer_texture.as_ref().unwrap().to_image(wgpu);
            img_data.save("data/depth_pass.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_pass_gray.png").unwrap();

            state.save_depth_pass_image = false;
        }

        if state.save_depth_buffer_image
        {
            let img_data = self.depth_buffer_texture.as_ref().unwrap().to_image(wgpu);
            img_data.save("data/depth_buffer.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_buffer_gray.png").unwrap();

            state.save_depth_buffer_image = false;
        }
    }

    pub fn msaa_sample_size_update(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, samples: u32)
    {
        self.samples = samples;
        self.create_pipelines(wgpu, scene, true);
    }

    pub fn resize(&mut self, wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>)
    {
        dbg!("resize");
        for cam in &mut scene.cameras
        {
            cam.init(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.init_matrices();
        }

        self.depth_buffer_texture = Some(Texture::new_depth_texture(wgpu, self.samples));
        self.depth_pass_buffer_texture = Some(Texture::new_depth_texture(wgpu, 1));
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

    pub fn render(&mut self, wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, scene: &Box<crate::state::scene::scene::Scene>) -> u32
    {
        let all_nodes = Scene::list_all_child_nodes(&scene.nodes);
        let mut read_nodes = vec![];

        for node in &all_nodes
        {
            read_nodes.push(node.read().unwrap());
        }

        let mut draw_calls: u32 = 0;
        draw_calls += self.render_depth(wgpu, view, msaa_view, encoder, &read_nodes);
        draw_calls += self.render_color(wgpu, view, msaa_view, encoder, &read_nodes);

        draw_calls
    }

    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>) -> u32
    {
        let clear_color = wgpu::Color::BLACK;

        // todo: replace with internal texture?
        let render_pass_view = view;

        let mut color_attachments: &[Option<RenderPassColorAttachment>] = &[
            Some(wgpu::RenderPassColorAttachment
            {
                view: render_pass_view,
                resolve_target: None,
                ops: wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: true,
                },
            })
        ];

        if !self.depth_pipe.as_ref().unwrap().fragment_attachment
        {
            color_attachments = &[];
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("depth pass"),
            color_attachments: color_attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_pass_buffer_texture.as_ref().unwrap().get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        self.draw_phase(&mut render_pass, &self.depth_pipe.as_ref().unwrap(), nodes)
    }

    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>) -> u32
    {
        let mut render_pass_view = view;
        let mut render_pass_resolve_target = None;
        if msaa_view.is_some()
        {
            render_pass_view = msaa_view.as_ref().unwrap();
            render_pass_resolve_target = Some(view);
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("color pass"),
            color_attachments:
            &[
                Some(wgpu::RenderPassColorAttachment
                {
                    view: render_pass_view,
                    resolve_target: render_pass_resolve_target,
                    ops: wgpu::Operations
                    {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: true,
                    },
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_buffer_texture.as_ref().unwrap().get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        self.draw_phase(&mut render_pass, &self.color_pipe.as_ref().unwrap(), nodes)
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