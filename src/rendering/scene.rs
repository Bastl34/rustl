use std::{sync::{RwLockReadGuard, Arc, RwLock}, mem::swap, vec};

use gltf::mesh::util::weights;
use nalgebra::{Point3, distance_squared};
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment, BindGroup, util::DeviceExt};

use crate::{component_downcast, component_downcast_mut, helper::image::float32_to_grayscale, render_item_impl_default, rendering::morph_target, resources::resources, state::{helper::render_item::{get_render_item, get_render_item_mut, RenderItem}, scene::{camera::CameraData, components::{self, alpha::Alpha, component::{Component, ComponentBox}, joint::{self, Joint}, material::TextureType, mesh::Mesh, transformation::Transformation}, node::{Node, NodeItem}, scene::SceneData}, state::State}};

use super::{wgpu::WGpu, pipeline::Pipeline, texture::{Texture, TextureFormat}, camera::CameraBuffer, instance::InstanceBuffer, vertex_buffer::VertexBuffer, light::LightBuffer, bind_groups::{light_cam_scene::LightCamSceneBindGroup, skeleton_morph_target::SkeletonMorphTargetBindGroup}, material::MaterialBuffer, helper::buffer::create_empty_buffer, skeleton::{self, SkeletonBuffer}, morph_target::MorphTarget};

type MaterialComponent = crate::state::scene::components::material::Material;
//type MeshComponent = crate::state::scene::components::mesh::Mesh;
type StateScene = crate::state::scene::scene::Scene;

pub struct RenderData<'a>
{
    node: &'a RwLockReadGuard<'a, Box<Node>>,
    material: &'a RwLockReadGuard<'a, ComponentBox>,
    meshes: &'a Vec<RwLockReadGuard<'a, ComponentBox>>,

    has_transparency: bool,
    alpha_index: u64,
    middle: Point3::<f32>
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneUniform
{
    pub gamma: f32,
    pub exposure: f32,
}

impl SceneUniform
{
    pub fn new(scene_data: &SceneData) -> Self
    {
        let gamma = if let Some(gamma) = scene_data.gamma { gamma } else { 0.0 };
        let exposure = if let Some(exposure) = scene_data.exposure { exposure } else { 0.0 };

        Self
        {
            gamma: gamma,
            exposure: exposure,
        }
    }
}

pub struct Scene
{
    clear_color: wgpu::Color,

    color_shader: String,
    depth_shader: String,

    samples: u32,
    pub distance_sorting: bool,

    depth_pipe: Option<Pipeline>,
    color_pipe: Option<Pipeline>,

    buffer: wgpu::Buffer,

    depth_pass_buffer_texture: Texture,
    depth_buffer_texture: Texture,

    empty_skeleton: SkeletonBuffer,
    empty_morph_target: MorphTarget,
    empty_skeleton_morph: SkeletonMorphTargetBindGroup,
}

impl RenderItem for Scene
{
    render_item_impl_default!();
}

impl Scene
{
    pub fn new(wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene, samples: u32) -> Scene
    {
        // shader source
        let color_shader = resources::load_string("shader/phong.wgsl").unwrap();
        let depth_shader = resources::load_string("shader/depth.wgsl").unwrap();

        let empty_skeleton = SkeletonBuffer::empty(wgpu);
        let empty_morph_target = MorphTarget::empty(wgpu);

        let empty_skeleton_morph = SkeletonMorphTargetBindGroup::new(wgpu, "empty", &empty_skeleton, &empty_morph_target);

        let mut render_scene = Self
        {
            clear_color: wgpu::Color::BLACK,

            color_shader,
            depth_shader,

            samples,
            distance_sorting: true,

            color_pipe: None,
            depth_pipe: None,

            buffer: create_empty_buffer(wgpu),

            depth_buffer_texture: Texture::new_depth_texture(wgpu, samples),
            depth_pass_buffer_texture: Texture::new_depth_texture(wgpu, 1),

            empty_skeleton,
            empty_morph_target,

            empty_skeleton_morph
        };

        render_scene.to_buffer(wgpu, scene);

        render_scene.update(wgpu, state, scene);
        render_scene.create_pipelines(wgpu, scene, false);

        render_scene
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, scene: &crate::state::scene::scene::Scene)
    {
        let data = scene.get_data();

        let scene_uniform = SceneUniform::new(data);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&scene.name),
                contents: bytemuck::cast_slice(&[scene_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, scene: &crate::state::scene::scene::Scene)
    {
        let data = scene.get_data();

        let scene_uniform = SceneUniform::new(data);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[scene_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, re_create: bool)
    {
        let light_cam_scene_bind_layout = LightCamSceneBindGroup::bind_layout(wgpu);

        // material and textures
        let mat = scene.get_default_material().unwrap();
        let mat = mat.read().unwrap();
        let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();

        let material_render_item = &mat.get_base().render_item;
        let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
        let material_bind_layout = material_render_item.bind_group_layout.as_ref().unwrap();

        let skeleton_morph_bind_layout = SkeletonMorphTargetBindGroup::bind_layout(wgpu);

        let bind_group_layouts =
        [
            material_bind_layout,
            &light_cam_scene_bind_layout,
            &skeleton_morph_bind_layout
        ];

        // ********** depth pass **********
        if !re_create
        {
            self.depth_pipe = Some(Pipeline::new(wgpu, "depth pipe", &self.depth_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, 1));
        }
        else
        {
            self.depth_pipe.as_mut().unwrap().re_create(wgpu, &bind_group_layouts, true, true, 1);
        }

        // ********** color pass **********
        let mut additional_textures = vec![];
        additional_textures.push(&self.depth_pass_buffer_texture);

        if !re_create
        {
            self.color_pipe = Some(Pipeline::new(wgpu, "color pipe", &self.color_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, self.samples));
        }
        else
        {
            self.color_pipe.as_mut().unwrap().re_create(wgpu, &bind_group_layouts, true, true, self.samples);
        }
    }

    pub fn update_textures(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene)
    {
        // check if the scene env texture has changed
        if let Some(env_tex) = &scene.get_data().environment_texture
        {
            if env_tex.enabled && env_tex.item.read().unwrap().get_data_tracker().changed()
            {
                dbg!("update all materials");
                let env_texture_id = env_tex.item.read().unwrap().id;

                for (_, material) in &mut scene.materials
                {
                    let mut material = material.write().unwrap();
                    let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

                    if !material.has_texture(TextureType::Environment) || material.has_texture_id(env_texture_id)
                    {
                        material.get_data_mut().force_change();
                    }
                }
            }
        }

        // check all individual textures
        for (_texture_id, texture) in &mut scene.textures
        {
            let mut buffer_recreate_needed = false;

            {
                let mut texture = texture.write().unwrap();
                let texture_changed = texture.get_data_mut().consume_change();

                // check if buffer recreation is needed
                // TODO: check if this even needed anymore (because of the changetracker data from texture)
                if let Some(render_item) = &texture.render_item
                {
                    let render_item = get_render_item::<Texture>(render_item);
                    buffer_recreate_needed = render_item.width != texture.width() || render_item.height != texture.height();
                }

                if texture.render_item.is_none() || buffer_recreate_needed || texture_changed
                {
                    let mut format = TextureFormat::Srgba;
                    if texture.channels() == 1
                    {
                        format = TextureFormat::Gray;
                    }

                    let render_item = Texture::new_from_texture(wgpu, texture.name.as_str(), &texture, format);
                    texture.render_item = Some(Box::new(render_item));
                    buffer_recreate_needed = true;
                }
                /*
                else if texture_changed
                {
                    let mut render_item = texture.render_item.take();

                    {
                        let render_item = get_render_item_mut::<Texture>(render_item.as_mut().unwrap());
                        render_item.update_buffer(wgpu, &texture);
                    }

                    texture.render_item = render_item;
                }
                 */
            }

            // mark material as "dirty" if the buffer needs a recreate
            if buffer_recreate_needed
            {
                let texture = texture.read().unwrap();

                for (_, material) in &mut scene.materials
                {
                    let mut material = material.write().unwrap();
                    let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

                    if material.has_texture_id(texture.id)
                    {
                        material.get_data_mut().force_change();
                    }
                }
            }
        }
    }

    pub fn update_materials(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, force: bool)
    {
        let default_env_map = scene.get_data().environment_texture.clone();

        for (_material_id, material) in &mut scene.materials
        {
            let mut material = material.write().unwrap();
            let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

            let material_changed = material.get_data_mut().consume_change();

            if material_changed || material.get_base().render_item.is_none()
            {
                dbg!("material render item recreate");
                let render_item: MaterialBuffer = MaterialBuffer::new(wgpu, &material, default_env_map.clone(), None);
                material.get_base_mut().render_item = Some(Box::new(render_item));
            }
            else if material_changed || force
            {
                let mut render_item = material.get_base_mut().render_item.take();

                {
                    let render_item = get_render_item_mut::<MaterialBuffer>(render_item.as_mut().unwrap());
                    render_item.to_buffer(wgpu, material, default_env_map.clone(), None);
                    render_item.create_binding_groups(wgpu, material, default_env_map.clone(), None);
                }

                material.get_base_mut().render_item = render_item;

                dbg!("material render item update");
            }
        }
    }

    pub fn update_light_cameras(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, force: bool)
    {
        // ********** lights: all **********
        let max_lights = scene.get_data().max_lights;
        let (lights, all_lights_changed) = scene.lights.consume_borrow();
        if all_lights_changed || force
        {
            if scene.lights_render_item.is_none()
            {
                let lights_buffer = LightBuffer::new(wgpu, format!("{} lights buffer", scene.name).to_string(), lights, max_lights);
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
            let cam_changed = cam.get_data_mut().consume_change();

            // create cam render item
            if cam.render_item.is_none()
            {
                cam.update_resolution(wgpu.surface_config().width, wgpu.surface_config().height);
                cam.init_matrices();

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

            // create cam/light/scene bind group
            if cam.bind_group_render_item.is_none() || all_lights_changed
            {
                let camera_buffer = get_render_item_mut::<CameraBuffer>(cam.render_item.as_mut().unwrap());
                let lights_buffer = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());

                let light_cam_scene_bind_group = LightCamSceneBindGroup::new(wgpu, &cam.name, &camera_buffer, &lights_buffer, &self);

                cam.bind_group_render_item = Some(Box::new(light_cam_scene_bind_group));
            }
        }
    }

    pub fn update_nodes(&mut self, wgpu: &mut WGpu, nodes: &mut Vec<Arc<RwLock<Box<Node>>>>)
    {
        //for node in scene.nodes.iter_mut()

        // go in reverse to find parent transformations for child nodes
        for node_id in (0..nodes.len()).rev()
        {
            let mut create_new_skeleton_morph_target_bind_group = false;

            // ********** vertex buffer and morph target/s **********
            {
                let node_arc = nodes.get(node_id).unwrap();

                let node = node_arc.read().unwrap();
                let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();

                if let Some(mesh) = mesh
                {
                    component_downcast_mut!(mesh, crate::state::scene::components::mesh::Mesh);

                    let mesh_data_changed = mesh.get_data_mut().consume_change();

                    if mesh_data_changed
                    {
                        let vertex_buffer = VertexBuffer::new(wgpu, "vertex buffer", mesh.get_data());
                        mesh.get_base_mut().render_item = Some(Box::new(vertex_buffer));

                        if MorphTarget::get_morph_targets(mesh.get_data()) > 0
                        {
                            let morph_target = MorphTarget::new(wgpu, "morph target", mesh.get_data());
                            mesh.morph_target_render_item = Some(Box::new(morph_target));

                            create_new_skeleton_morph_target_bind_group = true;
                        }
                    }

                    /*
                    let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
                    swap(&mut mesh.morph_target_render_item, &mut render_item);

                    drop(mesh);

                    //if let Some(morph_target_render_item) = &mesh.morph_target_render_item
                    if render_item.is_some()
                    {
                        if has_changed_morph_target_weights
                        {
                            let weights = node.get_morph_targets_vec();

                            if let Some(weights) = weights
                            {
                                let mut morph_render_item = get_render_item_mut::<MorphTarget>(render_item.as_mut().unwrap());
                                morph_render_item.update_buffer(wgpu, &weights);
                            }
                        }
                    }

                    let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();
                    let mesh = mesh.unwrap();
                    component_downcast_mut!(mesh, crate::state::scene::components::mesh::Mesh);

                    swap(&mut render_item, &mut mesh.morph_target_render_item);
                     */
                }
            }

            // ********** morph target/s **********
            {
                let node_arc = nodes.get(node_id).unwrap();

                let has_changed_morph_target_weights = Self::consume_changed_morph_targets(node_arc.clone());

                if has_changed_morph_target_weights
                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let node = node.read().unwrap();

                    let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();
                    if let Some(mesh) = mesh
                    {
                        let weights = node.get_morph_targets_vec();

                        if let Some(weights) = weights
                        {
                            component_downcast_mut!(mesh, crate::state::scene::components::mesh::Mesh);
                            let mut morph_render_item = get_render_item_mut::<MorphTarget>(mesh.morph_target_render_item.as_mut().unwrap());

                            morph_render_item.update_buffer(wgpu, &weights);
                        }
                    }
                }
            }

            // ********** skeleton **********
            {
                let node_arc = nodes.get_mut(node_id).unwrap();

                if node_arc.read().unwrap().skin.len() > 0
                {
                    if node_arc.read().unwrap().skeleton_render_item.is_none()
                    {
                        let mut node_write = node_arc.write().unwrap();

                        let joint_matrices = node_write.get_joint_transform_vec(true);
                        if let Some(joint_matrices) = joint_matrices
                        {
                            let skeleton_buffer = SkeletonBuffer::new(wgpu, "skeleton", &joint_matrices);
                            node_write.skeleton_render_item = Some(Box::new(skeleton_buffer));
                            create_new_skeleton_morph_target_bind_group = true;
                        }
                        else
                        {
                            let skeleton_buffer = SkeletonBuffer::new(wgpu, "skeleton", &vec![]);
                            node_write.skeleton_render_item = Some(Box::new(skeleton_buffer));
                            create_new_skeleton_morph_target_bind_group = true;
                        }
                    }
                    else if Self::has_changed_joints(node_arc.clone())
                    {
                        let joint_matrices = node_arc.read().unwrap().get_joint_transform_vec(true);
                        if let Some(joint_matrices) = joint_matrices
                        {
                            let mut node_write = node_arc.write().unwrap();
                            let render_item = get_render_item_mut::<SkeletonBuffer>(node_write.skeleton_render_item.as_mut().unwrap());
                            render_item.update_buffer(wgpu, &joint_matrices);
                        }
                    }
                }
            }

            // ********** skeleton and morph target/s bind group **********
            {
                let node = nodes.get_mut(node_id).unwrap();
                let mut node = node.write().unwrap();

                let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();

                if let Some(mesh) = mesh
                {
                    component_downcast!(mesh, crate::state::scene::components::mesh::Mesh);

                    let has_morph_targets = MorphTarget::get_morph_targets(mesh.get_data()) > 0;
                    let has_skeleton = node.skin.len() > 0;

                    if has_morph_targets || has_skeleton
                    {
                        if node.skeleton_morph_target_bind_group_render_item.is_none() || create_new_skeleton_morph_target_bind_group
                        {
                            // skeleton and morph targets
                            if has_morph_targets && has_skeleton
                            {
                                let skeleton_render_item = get_render_item::<SkeletonBuffer>(node.skeleton_render_item.as_ref().unwrap());
                                let morph_render_item = get_render_item::<MorphTarget>(mesh.morph_target_render_item.as_ref().unwrap());

                                let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Skeleton Morph Target", &skeleton_render_item, &morph_render_item);
                                node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                            }
                            // only skeleton
                            else if has_skeleton
                            {
                                let skeleton_render_item = get_render_item::<SkeletonBuffer>(node.skeleton_render_item.as_ref().unwrap());

                                let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Skeleton and Empty Morph Target", &skeleton_render_item, &self.empty_morph_target);
                                node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                            }
                            // only morph targets
                            else if has_morph_targets
                            {
                                let morph_render_item = get_render_item::<MorphTarget>(mesh.morph_target_render_item.as_ref().unwrap());

                                let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Empty Skeleton Morph Target", &self.empty_skeleton, &morph_render_item);
                                node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                            }
                        }
                    }
                }
            }

            // ********** instances all **********
            let mut all_instances_changed;
            {
                let node_arc = nodes.get_mut(node_id).unwrap();

                {
                    let mut write = node_arc.write().unwrap();
                    all_instances_changed = write.instances.consume_change();
                }

                {
                    let node = node_arc.write().unwrap();
                    let trans_component = node.find_component::<Transformation>();
                    if let Some(trans_component) = trans_component
                    {
                        component_downcast_mut!(trans_component, Transformation);
                        all_instances_changed = trans_component.get_data_mut().consume_change() || all_instances_changed;
                    }

                    if !all_instances_changed
                    {
                        let alpha_component = node.find_component::<Alpha>();
                        if let Some(alpha_component) = alpha_component
                        {
                            component_downcast_mut!(alpha_component, Alpha);
                            all_instances_changed = alpha_component.get_data_mut().consume_change() || all_instances_changed;
                        }
                    }
                }

                // check parents for changed transforms
                /*
                if !all_instances_changed
                {
                    all_instances_changed = Scene::find_changed_parent_data(node_arc.clone());
                }
                */

                if all_instances_changed
                {
                    //dbg!(" ============ instances updated");
                    let instance_buffer;
                    {
                        let node = node_arc.read().unwrap();
                        let instances = node.instances.get_ref();
                        instance_buffer = InstanceBuffer::new(wgpu, "instance buffer", instances);
                    }

                    node_arc.write().unwrap().instance_render_item = Some(Box::new(instance_buffer));
                }
            }

            // ********** mark instances as updated **********
            if all_instances_changed
            {
                {
                    let node = nodes.get(node_id).unwrap();
                    let node = node.read().unwrap();
                    let instances = node.instances.get_ref();

                    for instance in instances
                    {
                        let mut instance = instance.write().unwrap();
                        instance.get_data_mut().consume_change();
                    }
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
                    let node = nodes.get_mut(node_id).unwrap();
                    let mut node = node.write().unwrap();

                    swap(&mut node.instance_render_item, &mut render_item);
                }

                {
                    let node = nodes.get(node_id).unwrap();
                    let node = node.read().unwrap();
                    let instances_ref = node.instances.get_ref();

                    for (i, instance) in instances_ref.iter().enumerate()
                    {
                        let mut instance = instance.write().unwrap();
                        //let (instance_data, mut instance_changed) = instance.get_data_mut().consume_borrow();
                        let instance_changed = instance.get_data_mut().consume_change();

                        //instance_changed = Self::find_changed_instance_data(instance) || instance_changed;

                        if instance_changed
                        {
                            let render_item = get_render_item_mut::<InstanceBuffer>(render_item.as_mut().unwrap());
                            render_item.update_buffer(wgpu, &instance, i);

                            //dbg!(" ============ ONE instance updated");
                        }
                    }
                }

                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let mut node = node.write().unwrap();

                    swap(&mut render_item, &mut node.instance_render_item);
                }
            }
        }
    }

    pub fn consume_changed_morph_targets(node: Arc<RwLock<Box<Node>>>) -> bool
    {
        let node = node.read().unwrap();
        let morph_target_components = node.find_components::<components::morph_target::MorphTarget>();

        let mut has_changed = false;
        for morph_target in morph_target_components
        {
            component_downcast_mut!(morph_target, components::morph_target::MorphTarget);
            has_changed = morph_target.get_data_mut().consume_change() || has_changed;
        }

        has_changed
    }

    pub fn has_changed_joints(mesh_node: Arc<RwLock<Box<Node>>>) -> bool
    {
        let node = mesh_node.read().unwrap();

        for joint in &node.skin
        {
            let joint = joint.read().unwrap();
            let joint_component = joint.find_component::<Joint>();
            if let Some(joint_component) = joint_component
            {
                component_downcast!(joint_component, Joint);

                if joint_component.get_data_tracker().changed()
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn consume_changed_joints(nodes: &Vec<Arc<RwLock<Box<Node>>>>)
    {
        for node in nodes
        {
            let node = node.read().unwrap();
            let joint_component = node.find_component::<Joint>();

            if joint_component.is_none()
            {
                continue;
            }

            if let Some(joint_component) = joint_component
            {
                component_downcast_mut!(joint_component, Joint);

                joint_component.get_data_mut().consume_change();
            }
        }
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene)
    {
        // ********** clear color **********
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

        // ********** dynamic items **********
        self.update_textures(wgpu, scene);

        let scene_changed = scene.get_data_mut().consume_change();

        self.update_materials(wgpu, scene, scene_changed);
        self.update_light_cameras(wgpu, scene, scene_changed);

        if scene_changed
        {
            dbg!("scene data changed -> recreate materials/lights/pipelines");

            // update scene buffer
            self.update_buffer(wgpu, scene);

            // update pipelines
            self.create_pipelines(wgpu, scene, true);
        }

        let mut all_nodes = Scene::list_all_child_nodes(&scene.nodes, false);
        self.update_nodes(wgpu, &mut all_nodes);
        Self::consume_changed_joints(&all_nodes);

        // ********** screenshot stuff **********
        if state.save_image
        {
            let node_id = 0;
            let node_arc = scene.nodes.get(node_id).unwrap();

            let mat = node_arc.read().unwrap().find_component::<MaterialComponent>();

            if let Some(mat) = mat
            {
                component_downcast!(mat, MaterialComponent);

                let data = mat.get_data();

                {
                    let base_tex = data.texture_base.clone().unwrap();
                    let base_tex = base_tex.get().read().unwrap();
                    let render_item = base_tex.render_item.as_ref().unwrap();
                    let render_item = get_render_item::<Texture>(&render_item);

                    let img_data = render_item.to_image(wgpu);
                    img_data.save("data/base_texture.png").unwrap();
                }

                {
                    let base_tex = data.texture_normal.clone().unwrap();
                    let base_tex = base_tex.get().read().unwrap();
                    let render_item = base_tex.render_item.as_ref().unwrap();
                    let render_item = get_render_item::<Texture>(&render_item);

                    let img_data = render_item.to_image(wgpu);
                    img_data.save("data/normal_texture.png").unwrap();
                }
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

        self.depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples);

        //self.update_materials(wgpu, scene, true);
        self.create_pipelines(wgpu, scene, true);
    }

    pub fn resize(&mut self, wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>)
    {
        dbg!("resize");
        for cam in &mut scene.cameras
        {
            cam.update_resolution(wgpu.surface_config().width, wgpu.surface_config().height);
            cam.init_matrices();
        }

        self.depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples);
        self.depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1);
    }

    pub fn list_all_child_nodes(nodes: &Vec<NodeItem>, check_visibility: bool) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            if check_visibility
            {
                let node = node.read().unwrap();
                let visible = node.visible;

                if !visible
                {
                    continue;
                }
            }

            let child_nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes, check_visibility);

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
        let all_nodes = Scene::list_all_child_nodes(&scene.nodes, true);

        let mut nodes_read = vec![];
        let mut materials = vec![];
        let mut materials_read = vec![];
        let mut meshes = vec![];
        let mut meshes_read = vec![];

        let default_material = scene.get_default_material();

        if scene.get_default_material().is_none()
        {
            dbg!("default material not found -> please do not delete it");
            return 0;
        }

        let default_material_arc = default_material.unwrap();
        let default_material = &default_material_arc.read().unwrap();

        for node in &all_nodes
        {
            let read_node = node.read().unwrap();
            let mat = read_node.find_component::<MaterialComponent>();
            let node_meshes = read_node.get_meshes();

            if node_meshes.len() > 0
            {
                if let Some(mat) = mat
                {
                    nodes_read.push(read_node);
                    materials.push(mat);
                    meshes.push(node_meshes);
                }
                else
                {
                    nodes_read.push(read_node);
                    materials.push(default_material_arc.clone());
                    meshes.push(node_meshes);
                }
            }
        }

        for material in &materials
        {
            let material_read = material.read().unwrap();
            materials_read.push(material_read);
        }

        for mesh in &meshes
        {
            let mesh_read: Vec<_> = mesh.iter().map(|mesh_item| mesh_item.read().unwrap()).collect();
            meshes_read.push(mesh_read);
        }

        let mut render_data = Vec::with_capacity(materials_read.len());
        for (i, material) in materials_read.iter().enumerate()
        {
            let mat;
            if !material.is_enabled()
            {
                mat = default_material;
            }
            else
            {
                mat = material;
            }

            let node = nodes_read.get(i).unwrap();
            let meshes = meshes_read.get(i).unwrap();

            let mut item_middle = Point3::<f32>::new(0.0, 0.0, 0.0);

            // ***** get center for depth sorting (alpha blending)
            if self.distance_sorting
            {
                if meshes.len() == 0 || node.instances.get_ref().len() == 0
                {
                    continue;
                }


                let mut mesh_middle = Point3::<f32>::new(0.0, 0.0, 0.0);
                for mesh in meshes
                {
                    let mesh = mesh.as_any().downcast_ref::<Mesh>().unwrap();
                    let center = mesh.get_data().b_box.center();
                    mesh_middle.x += center.x;
                    mesh_middle.y += center.y;
                    mesh_middle.z += center.z;
                }

                let len_f32 = meshes.len() as f32;
                mesh_middle.x /= len_f32;
                mesh_middle.y /= len_f32;
                mesh_middle.z /= len_f32;


                if let Some(instance_render_item) = node.instance_render_item.as_ref()
                {
                    let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);

                    for transform in &instance_buffer.transformations
                    {
                        let p = transform.transform_point(&mesh_middle);
                        item_middle.x += p.x;
                        item_middle.y += p.y;
                        item_middle.z += p.z;
                    }

                    let len_f32 = instance_buffer.transformations.len() as f32;
                    item_middle.x /= len_f32;
                    item_middle.y /= len_f32;
                    item_middle.z /= len_f32;
                }
            }

            let has_transparency;
            {
                let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();
                has_transparency = mat.has_transparency();
            }

            render_data.push
            (
                RenderData
                {
                    node: nodes_read.get(i).unwrap(),
                    material: mat,
                    meshes: meshes,

                    has_transparency: has_transparency,
                    alpha_index: node.alpha_index,
                    middle: item_middle
                }
            );
        }

        let mut draw_calls: u32 = 0;

        let mut i = 0;
        for cam in &scene.cameras
        {
            if !cam.enabled { continue; }

            let cam_data = cam.get_data();

            // sort
            if self.distance_sorting
            {

                let cam_pos = cam_data.eye_pos;
                render_data.sort_by(|a, b|
                {
                    if a.has_transparency != b.has_transparency
                    {
                        b.has_transparency.cmp(&a.has_transparency)
                    }
                    else if a.alpha_index != b.alpha_index
                    {
                        a.alpha_index.cmp(&b.alpha_index)
                    }
                    else
                    {
                        // we do not need the exact distance here - squared is fine
                        let a_dist = distance_squared(&a.middle, &cam_pos);
                        let b_dist = distance_squared(&b.middle, &cam_pos);

                        b_dist.partial_cmp(&a_dist).unwrap()
                    }

                    /*
                    a.alpha_index.cmp(&b.alpha_index)
                    .then_with(|| b_dist.partial_cmp(&a_dist).unwrap())
                    */

                    //b.partial_cmp(&a).unwrap()
                });
            }

            let clear;
            if i == 0 { clear = true; } else { clear = false; }

            // get bind groups
            let bind_group_render_item = cam.bind_group_render_item.as_ref().unwrap();
            let bind_group_render_item = get_render_item::<LightCamSceneBindGroup>(bind_group_render_item);

            draw_calls += self.render_depth(wgpu, view, encoder, &render_data, cam_data, &bind_group_render_item.bind_group, clear);
            draw_calls += self.render_color(wgpu, view, msaa_view, encoder, &render_data, cam_data, &bind_group_render_item.bind_group, clear);

            i += 1;
        }

        draw_calls
    }

    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, nodes: &Vec<RenderData>, cam_data: &CameraData, light_cam_bind_group: &BindGroup, clear: bool) -> u32
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
                    store: wgpu::StoreOp::Store,
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
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let x = cam_data.viewport_x * cam_data.resolution_width as f32;
        let y = cam_data.viewport_y * cam_data.resolution_height as f32;

        let width = cam_data.viewport_width * cam_data.resolution_width as f32;
        let height = cam_data.viewport_height * cam_data.resolution_height as f32;

        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        self.draw_phase(&mut render_pass, &self.depth_pipe.as_ref().unwrap(), nodes, light_cam_bind_group)
    }

    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, nodes: &Vec<RenderData>, cam_data: &CameraData, light_cam_bind_group: &BindGroup, clear: bool) -> u32
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
                        store: wgpu::StoreOp::Store,
                    },
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: clear_depth,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        let x = cam_data.viewport_x * cam_data.resolution_width as f32;
        let y = cam_data.viewport_y * cam_data.resolution_height as f32;

        let width = cam_data.viewport_width * cam_data.resolution_width as f32;
        let height = cam_data.viewport_height * cam_data.resolution_height as f32;

        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        self.draw_phase(&mut render_pass, &self.color_pipe.as_ref().unwrap(), nodes, light_cam_bind_group)
    }

    fn draw_phase<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a Pipeline, nodes: &'a Vec<RenderData>, light_cam_bind_group: &'a BindGroup) -> u32
    {
        let mut draw_calls: u32 = 0;

        for data in nodes
        {
            let node = data.node;
            let meshes = data.meshes;
            let mat = data.material;

            if !node.visible
            {
                continue;
            }

            if meshes.len() == 0
            {
                continue;
            }

            let material_render_item = mat.get_base().render_item.as_ref();
            let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
            let material_bind_group = material_render_item.bind_group.as_ref().unwrap();

            for mesh in meshes
            {
                let mesh = mesh.as_any().downcast_ref::<Mesh>().unwrap();

                if !mesh.get_base().is_enabled
                {
                    continue;
                }

                if let Some(render_item) = mesh.get_base().render_item.as_ref()
                {
                    let vertex_buffer = get_render_item::<VertexBuffer>(&render_item);

                    let instance_render_item = node.instance_render_item.as_ref().unwrap();
                    let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);

                    pass.set_pipeline(&pipeline.get());
                    pass.set_bind_group(0, material_bind_group, &[]);
                    pass.set_bind_group(1, light_cam_bind_group, &[]);

                    // skeleton
                    let skeleton_morph_target_render_item = node.skeleton_morph_target_bind_group_render_item.as_ref();
                    if let Some(skeleton_morph_target_render_item) = skeleton_morph_target_render_item
                    {
                        let skeleton_morph_target_render_item = get_render_item::<SkeletonMorphTargetBindGroup>(skeleton_morph_target_render_item);
                        pass.set_bind_group(2, &skeleton_morph_target_render_item.as_ref().bind_group, &[]);
                    }
                    else
                    {
                        pass.set_bind_group(2, &self.empty_skeleton_morph.bind_group, &[]);
                    }

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