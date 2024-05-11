use std::{borrow::BorrowMut, cell::RefCell, collections::HashMap, mem::swap, sync::{Arc, RwLock}, vec};

use anyhow::Ok;
use nalgebra::Vector3;
use nalgebra::Point3;
use parry3d::query::Ray;

use crate::{component_downcast, component_downcast_mut, helper::{self, change_tracker::ChangeTracker, math::{self, approx_zero}}, input::input_manager::InputManager, output::audio_device::AudioDeviceItem, resources::resources, state::{helper::render_item::RenderItemOption, scene::components::{component::{self, Component}, sound::Sound}}};

use super::{camera::{Camera, CameraItem}, components::{component::ComponentItem, material::{Material, MaterialItem, TextureState}, mesh::Mesh, sound}, light::{Light, LightItem}, manager::id_manager::{IdManager, IdManagerItem}, node::{Node, NodeItem}, scene_controller::{generic_controller::GenericController, scene_controller::SceneControllerBox}, sound_source::{self, SoundSource, SoundSourceItem}, texture::{Texture, TextureItem}};

pub type SceneItem = Box<Scene>;

#[derive(Clone)]
pub struct ScenePickRes
{
    pub time_of_impact: f32,
    pub point: Point3<f32>,
    pub normal: Option<Vector3<f32>>,
    pub node: NodeItem,
    pub instance_id: u64,
    pub face_id: Option<u32>,
}

impl ScenePickRes
{
    pub fn new(time_of_impact: f32, point: Point3<f32>, normal: Option<Vector3<f32>>, node: NodeItem, instance_id: u64, face_id: Option<u32>) -> ScenePickRes
    {
        Self
        {
            time_of_impact,
            point,
            normal,
            node,
            instance_id,
            face_id
        }
    }
}


pub struct SceneData
{
    pub max_lights: u32,
    pub environment_texture: Option<TextureState>,
    pub gamma: Option<f32>,
    pub exposure: Option<f32>
}

pub struct Scene
{
    pub id_manager: IdManagerItem,

    pub id: u64,
    pub name: String,
    pub visible: bool,

    data: ChangeTracker<SceneData>,

    pub audio_device: AudioDeviceItem,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<CameraItem>,
    pub lights: ChangeTracker<Vec<RefCell<ChangeTracker<LightItem>>>>,
    pub textures: HashMap<String, TextureItem>,
    pub materials: HashMap<u64, MaterialItem>,
    pub sound_sources: HashMap<String, SoundSourceItem>,

    pub pre_controller: Vec<SceneControllerBox>, // before scene updates
    pub post_controller: Vec<SceneControllerBox>, // after scene updates

    pub render_item: RenderItemOption,
    pub lights_render_item: RenderItemOption,
}

impl Scene
{
    pub fn new(id: u64, name: &str, audio_device: AudioDeviceItem) -> Scene
    {
        Self
        {
            id_manager: Arc::new(RwLock::new(IdManager::new())),

            id: id,
            name: name.to_string(),
            visible: true,

            data: ChangeTracker::new(SceneData
            {
                max_lights: 10,
                environment_texture: None,
                gamma: None,
                exposure: None,
            }),

            audio_device,

            nodes: vec![],
            cameras: vec![],
            lights: ChangeTracker::new(vec![]),
            textures: HashMap::new(),
            materials: HashMap::new(),
            sound_sources: HashMap::new(),

            pre_controller: vec![],
            post_controller: vec![],

            render_item: None,
            lights_render_item: None,
        }
    }

    pub fn get_data(&self) -> &SceneData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<SceneData>
    {
        &mut self.data
    }

    pub fn update(&mut self, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64)
    {
        // check moved nodes (if a note has a parent -> remove it from scene nodes)
        // this can happen when a node parent was set via set_parent
        let mut nodes_to_remove = vec![];
        for node in &self.nodes
        {
            if node.read().unwrap().parent.is_some()
            {
                nodes_to_remove.push(node.clone());
            }
        }

        for node_to_remove in nodes_to_remove
        {
            self.nodes.retain(|node|
            {
                node.read().unwrap().id != node_to_remove.read().unwrap().id
            });
        }

        // update pre controller
        let mut pre_controller = vec![];
        swap(&mut self.pre_controller, &mut pre_controller);
        for controller_item in &mut pre_controller
        {
            if controller_item.get_base().is_enabled
            {
                controller_item.update(self, input_manager, frame_scale);
            }
        }

        swap(&mut pre_controller, &mut self.pre_controller);

        // update nodes
        for node in &self.nodes
        {
            Node::update(node.clone(), input_manager, time, frame_scale, frame);
        }

        // cameras
        let mut cameras = vec![];
        swap(&mut self.cameras, &mut cameras);
        for cam in &mut cameras
        {
            cam.update(self, input_manager, frame_scale);
        }

        swap(&mut cameras, &mut self.cameras);

        // update post controller
        let mut post_controller = vec![];
        swap(&mut self.post_controller, &mut post_controller);
        for controller_item in &mut post_controller
        {
            if controller_item.get_base().is_enabled
            {
                controller_item.update(self, input_manager, frame_scale);
            }
        }

        swap(&mut post_controller, &mut self.post_controller);
    }

    pub fn print(&self)
    {
        println!(" - (SCENE) id={} name={} nodes={} cameras={} lights={} materials={} textures={}", self.id, self.name, self.nodes.len(), self.cameras.len(), self.lights.get_ref().len(), self.materials.len(), self.textures.len());

        //nodes
        for node in &self.nodes
        {
            node.read().unwrap().print(2);
        }

        // cameras
        for cam in &self.cameras
        {
            cam.print_short();
        }

        // lights
        for light in self.lights.get_ref()
        {
            let light = light.borrow();
            let light = light.get_ref();
            light.print_short();
        }
    }

    pub fn add_node(&mut self, node: NodeItem)
    {
        self.nodes.push(node);
    }

    pub fn clear_nodes(&mut self)
    {
        self.nodes.clear();
    }

    pub async fn load_texture_or_reuse_async(&mut self, path: &str, extension: Option<String>, max_tex_res: u32) -> anyhow::Result<TextureItem>
    {
        let image_bytes = resources::load_binary_async(path).await?;

        Ok(self.load_texture_byte_or_reuse(&image_bytes, path, extension, max_tex_res))
    }

    pub fn load_texture_or_reuse(&mut self, path: &str, extension: Option<String>, max_tex_res: u32) -> anyhow::Result<TextureItem>
    {
        let image_bytes = resources::load_binary(path)?;

        Ok(self.load_texture_byte_or_reuse(&image_bytes, path, extension, max_tex_res))
    }

    pub fn load_texture_byte_or_reuse(&mut self, image_bytes: &Vec<u8>, name: &str, extension: Option<String>, max_tex_res: u32) -> TextureItem
    {
        let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);

        if self.textures.contains_key(&hash)
        {
            println!("reusing texture {}", name);
            return self.textures.get_mut(&hash).unwrap().clone();
        }

        let id = self.id_manager.write().unwrap().get_next_texture_id();
        let texture = Texture::new(id, name, &image_bytes, extension, max_tex_res);

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.textures.insert(hash, arc.clone());

        arc
    }

    pub fn load_sound_source_byte_or_reuse(&mut self, sound_bytes: &Vec<u8>, name: &str, extension: Option<String>) -> SoundSourceItem
    {
        let hash = helper::crypto::get_hash_from_byte_vec(&sound_bytes);

        if self.sound_sources.contains_key(&hash)
        {
            println!("reusing sound source {}", name);
            return self.sound_sources.get_mut(&hash).unwrap().clone();
        }

        let id = self.id_manager.write().unwrap().get_next_sound_source_id();
        let texture = SoundSource::new(id, name, self.audio_device.clone(), &sound_bytes, extension);

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.sound_sources.insert(hash, arc.clone());

        arc
    }

    pub fn insert_texture_or_reuse(&mut self, texture: Texture, name: &str) -> TextureItem
    {
        let hash = texture.hash.clone();

        if self.textures.contains_key(&hash)
        {
            println!("reusing texture {}", name);
            return self.textures.get_mut(&hash).unwrap().clone();
        }

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.textures.insert(hash, arc.clone());

        arc
    }

    fn clear_empty_nodes_recursive(nodes: &mut Vec<NodeItem>)
    {
        nodes.retain(|node|
        {
            let node = node.read().unwrap();
            let is_empty = node.is_empty();

            !is_empty
        });

        for node in nodes
        {
            let mut node = node.write().unwrap();
            Self::clear_empty_nodes_recursive(&mut node.nodes);
        }
    }

    pub fn clear(&mut self)
    {
        self.nodes.clear();
        self.lights.get_mut().clear();
        self.cameras.clear();

        self.textures.clear();
        self.materials.clear();

        // re-add defaults
        self.add_default_material();

        if let Some(env_texture) = &self.get_data().environment_texture
        {
            let hash = env_texture.item.read().unwrap().hash.clone();
            self.textures.insert(hash, env_texture.item.clone());
        }
    }

    pub fn add_defaults(&mut self)
    {
        self.add_default_material();

        // post controller
        let mut controller = GenericController::default();
        self.post_controller.push(Box::new(controller));
    }

    pub fn clear_empty_nodes(&mut self)
    {
        Self::clear_empty_nodes_recursive(&mut self.nodes);
    }

    pub fn add_material(&mut self, id: u64, material: &MaterialItem)
    {
        self.materials.insert(id, material.clone());
    }

    pub fn add_default_material(&mut self)
    {
        let material_id = self.id_manager.write().unwrap().get_next_component_id();
        let material = Material::new(material_id, "default");

        let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
        self.add_material(material_id, &material_arc);
    }

    pub fn get_default_material(&self) -> Option<MaterialItem>
    {
        for (_, material) in &self.materials
        {
            if material.read().unwrap().get_base().name == "default"
            {
                return Some(material.clone());
            }
        }

        None
    }

    pub fn get_material_by_id(&self, id: u64) -> Option<MaterialItem>
    {
        if self.materials.contains_key(&id)
        {
            let item = self.materials.get(&id).unwrap();
            return Some(item.clone());
        }

        None
    }

    pub fn get_material_by_name(&self, name: &str) -> Option<MaterialItem>
    {
        for material in &self.materials
        {
            let material = material.1;
            if material.read().unwrap().get_base().name == name
            {
                return Some(material.clone());
            };
        }

        None
    }

    pub fn get_material_or_default(&self, node: NodeItem) -> Option<MaterialItem>
    {
        let node = node.read().unwrap();
        let mut material = node.find_component::<Material>();

        if material.is_none()
        {
            material = self.get_default_material();
        }

        material
    }

    pub fn delete_material_by_id(&mut self, id: u64) -> bool
    {
        // remove material from all nodes
        let all_nodes = Self::list_all_child_nodes(&self.nodes);

        for node in all_nodes
        {
            let mut node = node.write().unwrap();
            node.remove_component_by_id(id);

            for instance in node.instances.get_ref()
            {
                if instance.read().unwrap().find_component_by_id(id).is_some()
                {
                    instance.write().unwrap().remove_component_by_id(id);
                }
            }
        }

        let len = self.materials.len();
        self.materials.remove(&id);

        if self.materials.len() != len
        {
            return true;
        }

        false
    }

    pub fn get_texture_by_id(&self, id: u64) -> Option<TextureItem>
    {
        for texture_arc in self.textures.values()
        {
            let texture =  texture_arc.read().unwrap();
            if texture.id == id
            {
                return Some(texture_arc.clone());
            }
        }

        None
    }

    pub fn delete_texture_by_id(&mut self, id: u64) -> bool
    {
        // remove texture from all materials
        for material in &mut self.materials
        {
            let material = material.1;
            component_downcast_mut!(material, Material);
            material.remove_texture_by_id(id);
        }

        let len = self.textures.len();
        self.textures.retain(|_key, texture|
        {
            let texture = texture.read().unwrap();
            texture.id != id
        });

        self.textures.len() != len
    }

    pub fn get_sound_source_by_id(&self, id: u64) -> Option<SoundSourceItem>
    {
        for sound_arc in self.sound_sources.values()
        {
            let sound =  sound_arc.read().unwrap();
            if sound.id == id
            {
                return Some(sound_arc.clone());
            }
        }

        None
    }

    pub fn delete_sound_source_by_id(&mut self, id: u64) -> bool
    {
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);

        // remove sound component from all nodes
        for node in all_nodes
        {
            let mut node = node.write().unwrap();

            node.components.retain(|component|
            {
                let component = component.read().unwrap();

                if let Some(sound) = component.as_any().downcast_ref::<Sound>()
                {
                    if let Some(sound_source) = &sound.sound_source
                    {
                        let sound_source = sound_source.read().unwrap();
                        if sound_source.id == id
                        {
                            return false;
                        }
                    }
                }

                true
            });

            for instance in node.instances.get_mut()
            {
                let mut instance = instance.write().unwrap();

                instance.components.retain(|component|
                {
                    let component = component.read().unwrap();

                    if let Some(sound) = component.as_any().downcast_ref::<Sound>()
                    {
                        if let Some(sound_source) = &sound.sound_source
                        {
                            let sound_source = sound_source.read().unwrap();
                            if sound_source.id == id
                            {
                                return false;
                            }
                        }
                    }

                    true
                });
            }
        }

        // remove sound source
        let len = self.sound_sources.len();
        self.sound_sources.retain(|_key, sound|
        {
            let sound = sound.read().unwrap();
            sound.id != id
        });

        self.sound_sources.len() != len
    }

    pub fn get_sound_by_id(&self, id: u64) -> Option<ComponentItem>
    {
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);

        for node in all_nodes
        {
            let node = node.read().unwrap();

            if let Some(component) = node.find_component_by_id(id)
            {
                return Some(component.clone());
            }

            for instance in node.instances.get_ref()
            {
                let instance = instance.read().unwrap();

                if let Some(component) = instance.find_component_by_id(id)
                {
                    return Some(component.clone());
                }
            }
        }

        None
    }

    pub fn get_camera_by_id(&self, id: u64) -> Option<&CameraItem>
    {
        self.cameras.iter().find(|cam|{ cam.id == id })
    }

    pub fn get_camera_by_id_mut(&mut self, id: u64) -> Option<&mut CameraItem>
    {
        self.cameras.iter_mut().find(|cam|{ cam.id == id })
    }

    pub fn delete_camera_by_id(&mut self, id: u64) -> bool
    {
        let len = self.cameras.len();
        self.cameras.retain(|camera|
        {
            camera.id != id
        });

        self.cameras.len() != len
    }

    pub fn add_camera(&mut self, name: &str) -> &CameraItem
    {
        let cam = Camera::new(self.id_manager.write().unwrap().get_next_camera_id(), name.to_string());
        self.cameras.push(Box::new(cam));

        self.cameras.last().unwrap()
    }

    //pub fn get_active_camera() -> Option<&'static CameraItem>
    pub fn get_active_camera(&self) -> Option<&CameraItem>
    {
        for camera in &self.cameras
        {
            if camera.enabled
            {
                return Some(camera);
            }
        }
        None
    }

    pub fn get_active_camera_mut(&mut self) -> Option<&mut CameraItem>
    {
        for camera in self.cameras.iter_mut()
        {
            if camera.enabled
            {
                return Some(camera);
            }
        }
        None
    }

    pub fn get_light_by_id(&self, id: u64) -> Option<&RefCell<ChangeTracker<Box<Light>>>>
    {
        let lights = self.lights.get_ref();
        lights.iter().find(|light|{ light.borrow().get_ref().id == id })
    }

    pub fn delete_light_by_id(&mut self, id: u64) -> bool
    {
        // only mark as changed if there was a change
        let lights = self.lights.get_unmarked_mut();

        let len = lights.len();
        lights.retain(|light|
        {
            light.borrow().get_ref().id != id
        });

        if lights.len() != len
        {
            // only mark as changed if there was a change
            self.lights.force_change();
            return true;
        }

        false
    }

    pub fn add_light_point(&mut self, name: &str, pos: Point3<f32>, color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_point(self.id_manager.write().unwrap().get_next_light_id(), name.to_string(), pos, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_directional(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_directional(self.id_manager.write().unwrap().get_next_light_id(), name.to_string(), pos, dir, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_spot(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, max_angle: f32, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_spot(self.id_manager.write().unwrap().get_next_light_id(), name.to_string(), pos, dir, color, max_angle, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn list_all_child_nodes(nodes: &Vec<NodeItem>) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            let child_nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes);

            all_nodes.push(node.clone());
            all_nodes.extend(child_nodes);
        }

        all_nodes
    }

    pub fn list_all_child_nodes_with_mesh(nodes: &Vec<NodeItem>) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            let child_nodes = Scene::list_all_child_nodes_with_mesh(&node.read().unwrap().nodes);

            if node.read().unwrap().find_component::<Mesh>().is_some()
            {
                all_nodes.push(node.clone());
            }
            all_nodes.extend(child_nodes);
        }

        all_nodes
    }

    pub fn find_node_by_id(&self, id: u64) -> Option<NodeItem>
    {
        Node::find_node_by_id(&self.nodes, id)
    }

    pub fn find_node_by_name(&self, name: &str) -> Option<NodeItem>
    {
        Node::find_node_by_name(&self.nodes, name)
    }

    pub fn find_mesh_node_by_name(&self, name: &str) -> Option<NodeItem>
    {
        Node::find_mesh_node_by_name(&self.nodes, name)
    }

    pub fn delete_node_by_id(&mut self, id: u64) -> bool
    {
        // check camera targets and remove
        for camera in &mut self.cameras
        {
            if let Some(cam_node) = camera.node.clone()
            {
                if cam_node.read().unwrap().id == id
                {
                    camera.node = None;
                }
            }
        }

        let len = self.nodes.len();
        self.nodes.retain(|node|
        {
            node.read().unwrap().id != id
        });

        if self.nodes.len() != len
        {
            return true;
        }

        // if not found -> check children
        for node in &self.nodes
        {
            let deleted = node.write().unwrap().delete_node_by_id(id);

            if deleted
            {
                return true;
            }
        }

        false
    }

    pub fn multi_pick_node(&self, node: NodeItem, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, predicate: Option<Box<dyn Fn(NodeItem) -> bool>>) -> Vec<ScenePickRes>
    {
        let mut nodes = vec![];

        // check node itself
        if node.read().unwrap().find_component::<Mesh>().is_some()
        {
            nodes.push(node.clone());
        }

        // check child meshes/nodes
        let child_nodes_with_meshes = Scene::list_all_child_nodes_with_mesh(&node.read().unwrap().nodes);
        nodes.extend(child_nodes_with_meshes);

        self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, predicate)
    }

    pub fn pick_node(&self, node: NodeItem, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, predicate: Option<Box<dyn Fn(NodeItem) -> bool>>) -> Option<ScenePickRes>
    {
        let hits = self.multi_pick_node(node, ray, stop_on_first_hit, bounding_box_only, predicate);

        if hits.len() > 0
        {
            return Some(hits.first().unwrap().clone());
        }

        None
    }

    pub fn pick(&self, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, predicate: Option<Box<dyn Fn(NodeItem) -> bool>>) -> Option<ScenePickRes>
    {
        let nodes = Scene::list_all_child_nodes_with_mesh(&self.nodes);

        let hits = self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, predicate);

        if hits.len() > 0
        {
            return Some(hits.first().unwrap().clone());
        }

        None
    }

    pub fn multi_pick(&self, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, predicate: Option<Box<dyn Fn(NodeItem) -> bool>>) -> Vec<ScenePickRes>
    {
        let nodes = Scene::list_all_child_nodes_with_mesh(&self.nodes);

        self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, predicate)
    }

    fn pick_nodes(&self, nodes: &Vec<Arc<RwLock<Box<Node>>>>, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, predicate: Option<Box<dyn Fn(NodeItem) -> bool>>) -> Vec<ScenePickRes>
    {
        // find hits (bbox based)
        let mut hits_bbox = vec![];

        let mut no_bbox_picking_items = vec![];

        'outer: for node_arc in nodes
        {
            let node = node_arc.read().unwrap();

            // early "return" check
            if !node.visible
            {
                continue;
            }

            // mesh
            let mesh = node.find_component::<Mesh>();

            if mesh.is_none()
            {
                continue;
            }

            let mesh = mesh.unwrap();
            component_downcast!(mesh, Mesh);

            if !mesh.get_base().is_enabled
            {
                continue;
            }

            //if let Some(ref predicate) = predicate
            if let Some(predicate) = &predicate
            {
                if !predicate(node_arc.clone())
                {
                    continue;
                }
            }

            for instance in node.instances.get_ref()
            {
                let instance = instance.read().unwrap();

                if !instance.pickable
                {
                    continue;
                }

                let alpha = instance.get_alpha();

                if approx_zero(alpha)
                {
                    continue;
                }

                // transformation
                let transform = instance.get_world_transform();
                let transform_inverse = transform.try_inverse().unwrap();

                let ray_inverse = math::inverse_ray(ray, &transform_inverse);

                if !node.pick_bbox_first
                {
                    no_bbox_picking_items.push((node_arc, instance.id, transform, transform_inverse, ray_inverse));
                }
                else
                {
                    let solid = true;
                    let dist = mesh.intersect_b_box(&ray_inverse, solid);
                    if let Some(dist) = dist
                    {
                        hits_bbox.push((node_arc, instance.id, dist, transform, transform_inverse, ray_inverse));
                    }
                }

                if stop_on_first_hit && bounding_box_only && hits_bbox.len() > 0
                {
                    break 'outer;
                }
            }
        }

        if hits_bbox.len() == 0 && no_bbox_picking_items.len() == 0
        {
            return vec![];
        }

        // sort bbox dist (to get the nearest)
        hits_bbox.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

        if bounding_box_only && hits_bbox.len() > 0
        {
            let mut res = vec![];

            for hit_bbox in &hits_bbox
            {
                let node = hit_bbox.0;
                let instance = hit_bbox.1;
                let dist = hit_bbox.2;

                let pos = ray.origin + (ray.dir * dist);

                res.push(ScenePickRes::new(dist, pos, None, node.clone(), instance, None));

                if stop_on_first_hit
                {
                    return res;
                }
            }

            return res;

            /*
            let first = hits_bbox.first().unwrap();
            let node = first.0;
            let instance = first.1;
            let dist = first.2;

            //let dir = first.3 * (ray.dir.normalize() * dist).to_homogeneous();
            //let pos = ray.origin + dir.xyz();

            let pos = ray.origin + (ray.dir * dist);

            dbg!(" intersection 1");
            dbg!(node.read().unwrap().name.clone());

            //return Some((dist, pos, None, node.clone(), instance, None));
            return
             */
        }

        // combine bbox hits and nodes without bbox picking
        let mut ray_intersection_checks = vec![];
        for (node_arc, instance_id, _dist, transform, transform_inverse, ray_inverse) in hits_bbox
        {
            ray_intersection_checks.push((node_arc, instance_id, transform, transform_inverse, ray_inverse));
        }

        for (node_arc, instance_id, transform, transform_inverse, ray_inverse) in no_bbox_picking_items
        {
            ray_intersection_checks.push((node_arc, instance_id, transform, transform_inverse, ray_inverse));
        }

        // mesh based intersection
        let mut hits: Vec<ScenePickRes> = Vec::new();

        for (node_arc, instance_id, transform, transform_inverse, ray_inverse) in ray_intersection_checks
        {
            let node = node_arc.read().unwrap();

            let mesh = node.find_component::<Mesh>().unwrap();
            component_downcast!(mesh, Mesh);

            let material = self.get_material_or_default(node_arc.clone());
            let material = material.unwrap();
            component_downcast!(material, Material);
            let material_data = material.get_data();

            let solid = !material_data.backface_cullig;

            let mut joint_matrices = vec![];
            if node.skin.len() > 0
            {
                let matrices = node.get_joint_transform_vec(true);

                if let Some(matrices) = matrices
                {
                    joint_matrices = matrices;
                }
            }

            let intersection = mesh.intersect_skinned(ray, &ray_inverse, &transform, &transform_inverse, &joint_matrices, solid, material_data.smooth_shading);

            if let Some(intersection) = intersection
            {
                let pos = ray.origin + (ray.dir * intersection.0);

                hits.push(ScenePickRes::new(intersection.0, pos, Some(intersection.1), node_arc.clone(), instance_id, Some(intersection.2)));

                //if best_hit.is_none() || best_hit.is_some() && intersection.0 < best_hit.unwrap().0
                /*
                if best_hit.is_none()
                {
                    let pos = ray.origin + (ray.dir * intersection.0);

                    dbg!(" intersection 2");

                    //let dir = transform* (ray.dir.normalize() * intersection.0).to_homogeneous();
                    //let pos = ray.origin + dir.xyz();

                    best_hit = Some((intersection.0, pos, Some(intersection.1), node_arc.clone(), instance_id, Some(intersection.2)));
                }
                else if let Some(current_best_hit) = &best_hit
                {
                    if intersection.0 < current_best_hit.0
                    {
                        let pos = ray.origin + (ray.dir * intersection.0);

                        dbg!(" intersection 3");

                        //let dir = transform* (ray.dir.normalize() * intersection.0).to_homogeneous();
                        //let pos = ray.origin + dir.xyz();

                        best_hit = Some((intersection.0, pos, Some(intersection.1), node_arc.clone(), instance_id, Some(intersection.2)));
                    }
                }
                */
            }

            //if it should return on first hit
            //if best_hit.is_some() && stop_on_first_hit
            if hits.len() > 0 && stop_on_first_hit
            {
                return hits;
            }
        }

        // sort by distance
        hits.sort_by(|a, b| a.time_of_impact.partial_cmp(&b.time_of_impact).unwrap());

        // best_hit
        hits
    }

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui|
        {
            ui.label("name: ");
            ui.text_edit_singleline(&mut self.name);
        });

        ui.checkbox(&mut self.visible, "visible");

        let mut max_lights = self.get_data().max_lights;
        let mut gamma = if let Some(gamma_val) = self.get_data().gamma { gamma_val } else { 0.0 };
        let mut exposure = if let Some(exposure_val) = self.get_data().exposure { exposure_val } else { 0.0 };

        ui.horizontal(|ui|
        {
            ui.label("Max lights:");

            if ui.add(egui::DragValue::new(&mut max_lights).clamp_range(0..=20)).changed()
            {
                let data = self.get_data_mut().get_mut();

                data.max_lights = max_lights;
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Gamma:");

            if ui.add(egui::DragValue::new(&mut gamma).clamp_range(0.0..=10.0).speed(0.1)).changed()
            {
                let data = self.get_data_mut().get_mut();

                if approx_zero(gamma)
                {
                    data.gamma = None;
                }
                else
                {
                    data.gamma = Some(gamma);
                }
            }

            ui.label(" (sRGB: 2.2)");
        });

        ui.horizontal(|ui|
        {
            ui.label("Exposure:");

            if ui.add(egui::DragValue::new(&mut exposure).clamp_range(0.0..=100.0).speed(0.1)).changed()
            {
                let data = self.get_data_mut().get_mut();

                if approx_zero(exposure)
                {
                    data.exposure = None;
                }
                else
                {
                    data.exposure = Some(exposure);
                }
            }
        });
    }
}