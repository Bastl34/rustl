use std::{sync::RwLockReadGuard, mem::swap, ops::Deref};

use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment, BindGroup};

use crate::{state::{state::{State}, scene::{components::{component::Component, transformation::Transformation}, node::{Node, NodeItem}, camera::Camera}, helper::render_item::{get_render_item, get_render_item_mut, RenderItem}}, helper::image::float32_to_grayscale, resources::resources, render_item_impl_default};

use super::{wgpu::{WGpu}, pipeline::Pipeline, texture::Texture, camera::CameraBuffer, instance::{InstanceBuffer}, vertex_buffer::VertexBuffer, light::LightBuffer, bind_groups::light_cam::LightCamBindGroup, material::MaterialBuffer};

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

    depth_pass_buffer_texture: Texture,
    depth_buffer_texture: Texture,
}

impl RenderItem for Scene
{
    render_item_impl_default!();
}

impl Scene
{
    pub async fn new(wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene, samples: u32) -> Scene
    {
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

            depth_buffer_texture: Texture::new_depth_texture(wgpu, samples),
            depth_pass_buffer_texture: Texture::new_depth_texture(wgpu, 1),
        };

        render_scene.update(wgpu, state, scene);
        render_scene.create_pipelines(wgpu, scene, false);

        render_scene
    }

    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, re_create: bool)
    {
        /*
        let node_id = 0;
        let cam_id = 0;

        let node = scene.nodes.get_mut(node_id).unwrap();

        // material and textures
        let mat = node.write().unwrap().find_shared_component::<MaterialComponent>().unwrap();
        let mut mat = mat.write().unwrap();
        let mat = mat.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();
        let mat_data = mat.get_data_mut().get_mut();

        //let mut base_tex = mat_data.texture_base.as_mut().unwrap().write().unwrap();
        //let mut normal_tex = mat_data.texture_normal.as_mut().unwrap().write().unwrap();

        //let base_texture = Texture::new_from_texture(wgpu, base_tex.name.as_str(), &base_tex, true);
        //let normal_texture = Texture::new_from_texture(wgpu, normal_tex.name.as_str(), &normal_tex, false);

        let base_tex = mat_data.texture_base.as_mut().unwrap().read().unwrap();
        let normal_tex = mat_data.texture_normal.as_mut().unwrap().read().unwrap();

        //let base_texture = base_tex.render_item;
        //let normal_texture = normal_tex.render_item;

        let base_texture = get_render_item::<Texture>(base_tex.render_item.as_ref().unwrap());
        let normal_texture = get_render_item::<Texture>(normal_tex.render_item.as_ref().unwrap());

        let depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples);
        let depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1);

        // cam/light
        let cam = scene.cameras.get(cam_id).unwrap(); // TODO
        let cam = cam.borrow();
        let light_cam_bind_group = get_render_item::<LightCamBindGroup>(cam.get_ref().bind_group_render_item.as_ref().unwrap());

        // ********** depth pass **********
        let mut textures = vec![];
        textures.push(*base_texture);
        textures.push(*normal_texture);
        */


        // cam/light
        /*
        let cam_id = 0; // use first cam for bind group layout
        let cam = scene.cameras.get(cam_id).unwrap(); // TODO
        let cam = cam.borrow();
        let light_cam_bind_group = get_render_item::<LightCamBindGroup>(cam.get_ref().bind_group_render_item.as_ref().unwrap());
        */

        let light_cam_bind_layout = LightCamBindGroup::bind_layout(wgpu);

        let node_id = 0;

        let node = scene.nodes.get_mut(node_id).unwrap();

        // material and textures
        let mat = node.read().unwrap().find_shared_component::<MaterialComponent>().unwrap();
        let mat = mat.read().unwrap();
        let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();

        let material_render_item = &mat.get_base().render_item;
        let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
        let material_bind_layout = material_render_item.bind_group_layout.as_ref().unwrap();

        let bind_group_layouts =
        [
            material_bind_layout,
            &light_cam_bind_layout
        ];

        // ********** depth pass **********
        if !re_create
        {
            self.depth_pipe = Some(Pipeline::new(wgpu, "depth pipe", &self.depth_shader, &bind_group_layouts, scene.max_lights, true, true, 1));
        }
        else
        {
            self.depth_pipe.as_mut().unwrap().re_create(wgpu, &bind_group_layouts, true, true, 1);
        }

        // ********** color pass **********
        //textures.push(&depth_pass_buffer_texture);
        let mut additional_textures = vec![];
        additional_textures.push(&self.depth_pass_buffer_texture);

        if !re_create
        {
            self.color_pipe = Some(Pipeline::new(wgpu, "color pipe", &self.color_shader, &bind_group_layouts, scene.max_lights, true, true, self.samples));
        }
        else
        {
            self.color_pipe.as_mut().unwrap().re_create(wgpu, &bind_group_layouts, true, true, self.samples);
        }

        //base_tex.render_item = Some(Box::new(base_texture));
        //normal_tex.render_item = Some(Box::new(normal_texture));

        //self.depth_buffer_texture = Some(depth_buffer_texture);
        //self.depth_pass_buffer_texture = Some(depth_pass_buffer_texture);
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene)
    {
        let node_id = 0;

        let (clear_color, clear_color_changed) = state.rendering.clear_color.consume_borrow();

        if clear_color_changed
        {
            self.clear_color = wgpu::Color
            {
                r: clear_color.x as f64,
                g: clear_color.y as f64,
                b: clear_color.z as f64,
                a: 1.0,
            };
        }

        // ********** textures **********
        for (_texture_id, texture) in &mut scene.textures
        {
            let mut texture = texture.write().unwrap();
            if texture.render_item.is_none()
            {
                let render_item = Texture::new_from_texture(wgpu, texture.name.as_str(), &texture, true);
                texture.render_item = Some(Box::new(render_item));
            }
        }

        // ********** materials **********
        for (_material_id, material) in &mut scene.materials
        {
            let mut material = material.write().unwrap();
            let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

            let material_changed = material.get_data_mut().consume_change();

            if material_changed && material.get_base().render_item.is_none()
            {
                dbg!("material render item recreate");
                let render_item: MaterialBuffer = MaterialBuffer::new(wgpu, &material, None);
                material.get_base_mut().render_item = Some(Box::new(render_item));
            }
            else if material_changed
            {
                let mut render_item = material.get_base_mut().render_item.take();

                {
                    let render_item = get_render_item_mut::<MaterialBuffer>(render_item.as_mut().unwrap());
                    render_item.to_buffer(wgpu, material, None);
                    render_item.create_binding_groups(wgpu, material, None);
                }

                material.get_base_mut().render_item = render_item;

                dbg!("material render item update");
            }
        }

        // ********** lights: all **********
        let (lights, all_lights_changed) = scene.lights.consume_borrow();
        if all_lights_changed
        {
            if scene.lights_render_item.is_none()
            {
                let lights_buffer = LightBuffer::new(wgpu, format!("{} lights buffer", scene.name).to_string(), lights, scene.max_lights);
                scene.lights_render_item = Some(Box::new(lights_buffer));
            }

            let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
            render_item.to_buffer(wgpu, lights);

            //dbg!(" ============ lights updated");
        }

        // ********** light: check each **********
        if !all_lights_changed
        {
            for (i, light) in lights.iter().enumerate()
            {
                let mut light = light.borrow_mut();
                let (light, light_changed) = light.consume_borrow();
                if light_changed
                {
                    let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, light, i);

                    //dbg!(" ============ ONE light updated");
                }
            }
        }

        // ********** lights and cameras **********
        for cam in &mut scene.cameras
        {
            let mut cam = cam.borrow_mut();
            let (cam, cam_changed) = cam.consume_borrow_mut();

            // create cam render item
            if cam.render_item.is_none()
            {
                let camera_buffer = CameraBuffer::new(wgpu, &cam);
                cam.render_item = Some(Box::new(camera_buffer));
            }
            else if cam_changed
            {
                let mut render_item = cam.render_item.take();

                {
                    let render_item = get_render_item_mut::<CameraBuffer>(render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, cam.as_ref());
                }

                cam.render_item = render_item;
            }

            // create cam/light bind group
            if cam.bind_group_render_item.is_none() || all_lights_changed
            {
                let camera_buffer = get_render_item_mut::<CameraBuffer>(cam.render_item.as_mut().unwrap());
                let lights_buffer = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());

                let light_cam_bind_group = LightCamBindGroup::new(wgpu, &cam.name, &camera_buffer, &lights_buffer);

                cam.bind_group_render_item = Some(Box::new(light_cam_bind_group));
            }
        }

        // ********** vertex buffer **********
        {
            let node = scene.nodes.get_mut(node_id).unwrap();

            let mut node = node.write().unwrap();
            let mesh = node.find_component_mut::<crate::state::scene::components::mesh::Mesh>().unwrap();

            let (mesh_data, mesh_data_changed) = mesh.get_data_mut().consume_borrow_mut();

            if mesh_data_changed
            {
                let vertex_buffer = VertexBuffer::new(wgpu, "vertex buffer", mesh_data);
                mesh.get_base_mut().render_item = Some(Box::new(vertex_buffer));
            }
        }

        // ********** instances all **********
        let mut all_instances_changed;
        {
            let node_arc = scene.nodes.get_mut(node_id).unwrap();

            {
                let mut write = node_arc.write().unwrap();
                (_, all_instances_changed) = write.instances.consume_borrow();
            }

            {
                let mut node = node_arc.write().unwrap();
                let trans_component = node.find_component_mut::<Transformation>();
                if let Some(trans_component) = trans_component
                {
                    all_instances_changed = trans_component.get_data_mut().consume_change() || all_instances_changed;
                }
            }

            if all_instances_changed
            {
                //dbg!(" ============ instances updated");
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

            //let node_arc = scene.nodes.get_mut(node_id).unwrap();
        }

        // ********** instances check each **********
        if !all_instances_changed
        {
            let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
            {
                let node = scene.nodes.get_mut(node_id).unwrap();
                let mut node = node.write().unwrap();

                swap(&mut node.instance_render_item, &mut render_item);
            }

            {
                let node = scene.nodes.get(node_id).unwrap();
                let node = node.read().unwrap();
                let node_ref = node.instances.get_ref();

                for (i, instance) in node_ref.iter().enumerate()
                {
                    let mut instance = instance.borrow_mut();
                    let (instance, instance_changed) = instance.consume_borrow();
                    if instance_changed
                    {
                        let render_item = get_render_item_mut::<InstanceBuffer>(render_item.as_mut().unwrap());
                        render_item.update_buffer(wgpu, instance, i);

                        //dbg!(" ============ ONE instance updated");
                    }
                }
            }

            {
                let node = scene.nodes.get_mut(node_id).unwrap();
                let mut node = node.write().unwrap();

                swap(&mut render_item, &mut node.instance_render_item);
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
            cam.borrow_mut().get_mut().update_resolution(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.borrow_mut().get_mut().init_matrices();
        }

        self.depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples);
        self.depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1);
    }

    fn list_all_child_nodes(nodes: &Vec<NodeItem>) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            let child_nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes);

            if node.read().unwrap().render_children_first
            {
                all_nodes.extend(child_nodes);
                all_nodes.push(node.clone());
            }
            else
            {
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

        let mut i = 0;
        for cam in &scene.cameras
        {
            let cam = cam.borrow();
            let cam = cam.get_ref();
            if !cam.enabled { continue; }

            let clear;
            if i == 0 { clear = true; } else { clear = false; }

            // get bind groups
            let bind_group_render_item = cam.bind_group_render_item.as_ref().unwrap();
            let bind_group_render_item = get_render_item::<LightCamBindGroup>(bind_group_render_item);

            draw_calls += self.render_depth(wgpu, view, encoder, &read_nodes, cam, &bind_group_render_item.bind_group, clear);
            draw_calls += self.render_color(wgpu, view, msaa_view, encoder, &read_nodes, cam, &bind_group_render_item.bind_group, clear);

            i += 1;
        }

        draw_calls
    }

    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>, cam: &Box<Camera>, light_cam_bind_group: &BindGroup, clear: bool) -> u32
    {
        let mut clear_color = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let mut clear_depth = wgpu::LoadOp::Clear(1.0);

        if !clear
        {
            clear_color = wgpu::LoadOp::Load;
            clear_depth = wgpu::LoadOp::Load;
        }

        // todo: replace with internal texture?
        let render_pass_view = view;

        let mut color_attachments: &[Option<RenderPassColorAttachment>] = &[
            Some(wgpu::RenderPassColorAttachment
            {
                view: render_pass_view,
                resolve_target: None,
                ops: wgpu::Operations
                {
                    load: clear_color,
                    store: true,
                },
            })
        ];

        // TODO get rid of this
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
                view: &self.depth_pass_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: clear_depth,
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        let x = cam.viewport_x * cam.resolution_width as f32;
        let y = cam.viewport_y * cam.resolution_height as f32;

        let width = cam.viewport_width * cam.resolution_width as f32;
        let height = cam.viewport_height * cam.resolution_height as f32;

        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        self.draw_phase(&mut render_pass, &self.depth_pipe.as_ref().unwrap(), nodes, light_cam_bind_group)
    }

    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, nodes: &Vec<RwLockReadGuard<Box<Node>>>, cam: &Box<Camera>, light_cam_bind_group: &BindGroup, clear: bool) -> u32
    {
        let mut render_pass_view = view;
        let mut render_pass_resolve_target = None;
        if msaa_view.is_some()
        {
            render_pass_view = msaa_view.as_ref().unwrap();
            render_pass_resolve_target = Some(view);
        }

        let mut clear_color = wgpu::LoadOp::Clear(self.clear_color);
        let mut clear_depth = wgpu::LoadOp::Clear(1.0);

        if !clear
        {
            clear_color = wgpu::LoadOp::Load;
            clear_depth = wgpu::LoadOp::Load;
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
                        load: clear_color,
                        store: true,
                    },
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: clear_depth,
                    store: true,
                }),
                stencil_ops: None,
            })
        });

        let x = cam.viewport_x * cam.resolution_width as f32;
        let y = cam.viewport_y * cam.resolution_height as f32;

        let width = cam.viewport_width * cam.resolution_width as f32;
        let height = cam.viewport_height * cam.resolution_height as f32;

        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        self.draw_phase(&mut render_pass, &self.color_pipe.as_ref().unwrap(), nodes, light_cam_bind_group)
    }

    fn draw_phase<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a Pipeline, nodes: &'a Vec<RwLockReadGuard<Box<Node>>>, light_cam_bind_group: &'a BindGroup) -> u32
    {
        let mut draw_calls: u32 = 0;

        for node in nodes
        {
            let meshes = node.get_meshes();

            if meshes.is_none()
            {
                continue;
            }

            let mat = node.find_shared_component::<MaterialComponent>().unwrap();
            let mat = mat.read().unwrap();
            let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();

            let material_render_item = mat.get_base().render_item.as_ref();
            let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
            let material_bind_group = material_render_item.bind_group.as_ref().unwrap();

            let meshes = meshes.unwrap();

            for mesh in meshes
            {
                if let Some(render_item) = mesh.get_base().render_item.as_ref()
                {
                    let vertex_buffer = get_render_item::<VertexBuffer>(&render_item);

                    let instance_render_item = node.instance_render_item.as_ref().unwrap();
                    let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);

                    //let camera_render_item = cam.render_item.as_ref().unwrap();
                    //let camera_buffer = get_render_item::<CameraBuffer>(camera_render_item);

                    pass.set_pipeline(&pipeline.get());
                    //pass.set_bind_group(0, pipeline.get_textures_bind_group(), &[]);
                    pass.set_bind_group(0, material_bind_group, &[]);
                    //pass.set_bind_group(1, camera_buffer.get_bind_group(), &[]);
                    //pass.set_bind_group(1, bind_group, &[]);
                    pass.set_bind_group(1, light_cam_bind_group, &[]);
                    //pass.set_bind_group(2, pipeline.get_light_bind_group(), &[]);

                    pass.set_vertex_buffer(0, vertex_buffer.get_vertex_buffer().slice(..));

                    // instancing
                    pass.set_vertex_buffer(1, instance_buffer.get_buffer().slice(..));

                    pass.set_index_buffer(vertex_buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..vertex_buffer.get_index_count(), 0, 0..instance_buffer.get_count() as _);

                    draw_calls += 1;
                }
            }
        }

        draw_calls
    }

}