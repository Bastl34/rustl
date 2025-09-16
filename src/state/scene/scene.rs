#![allow(dead_code)]

use std::{cell::RefCell, collections::HashMap, fmt, mem::swap, sync::{Arc, RwLock}, vec};

use nalgebra::Vector3;
use nalgebra::Point3;
use parry3d::query::Ray;
use serde::{de::{MapAccess, Visitor}, ser::SerializeMap, Deserialize, Deserializer, Serialize, Serializer};
use wgpu::hal::auxil::db;

use crate::{component_downcast, component_downcast_mut, console_log, helper::{change_tracker::ChangeTracker, math::{self, approx_zero}, option_or_id::OptionOrId}, impl_arc_rwbox_map_serializer, state::{helper::render_item::RenderItemOption, resources::{mesh_resource::MeshResourceItem, sound_source::SoundSourceItem, texture::TextureItem}, scene::{components::{component::Component, sound::Sound}, manager::id_manager, utilities::tags}, state::{InputOutput, ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX}}};

use super::{camera::{Camera, CameraItem}, components::{component::ComponentItem, material::{Material, MaterialItem, TextureState}, mesh::Mesh}, light::{Light, LightItem}, node::{Node, NodeItem}, scene_controller::{generic_controller::GenericController, scene_controller::SceneControllerBox}};

pub type SceneItem = Box<Scene>;
pub type PickPredicate = Arc<dyn Fn(NodeItem, Option<u64>) -> bool>;

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


#[derive(Serialize, Deserialize)]
pub struct SceneData
{
    pub max_lights: u32,
    pub environment_texture: Option<TextureState>,
    pub gamma: Option<f32>,
    pub exposure: Option<f32>,
}

pub struct Scene
{
    pub id: u64,
    pub uuid: String,

    pub name: String,
    pub visible: bool,
    pub main: bool,

    data: ChangeTracker<SceneData>,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<CameraItem>,
    pub lights: ChangeTracker<Vec<RefCell<ChangeTracker<LightItem>>>>,
    pub materials: HashMap<u64, MaterialItem>,

    pub pre_controller: Vec<SceneControllerBox>, // before scene updates
    pub post_controller: Vec<SceneControllerBox>, // after scene updates

    pub render_item: RenderItemOption,
    pub lights_render_item: RenderItemOption,
}

impl Default for Scene
{
    fn default() -> Self
    {
        Scene::new("default")
    }
}

impl_arc_rwbox_map_serializer!(MaterialsSerializer, u64, dyn Component);

impl Serialize for Scene
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("uuid", &self.uuid)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("visible", &self.visible)?;
        map.serialize_entry("main", &self.main)?;
        map.serialize_entry("data", &self.data)?;

        let node_guards: Vec<_> = self.nodes.iter().map(|arc| arc.read().unwrap()).collect();
        let node_refs: Vec<&Node> = node_guards.iter().map(|guard| guard.as_ref()).collect();
        map.serialize_entry("nodes", &node_refs)?;

        let camera_refs: Vec<&Camera> = self.cameras.iter().map(|cam| cam.as_ref()).collect();
        map.serialize_entry("cameras", &camera_refs)?;

        let lights_guards: Vec<_> = self.lights.get_ref().iter().map(|cell| cell.borrow()).collect();
        let lights_refs: Vec<&Light> = lights_guards.iter().map(|tracker| tracker.get_ref().as_ref()).collect();
        map.serialize_entry("lights", &lights_refs)?;

        map.serialize_entry("materials", &MaterialsSerializer { map: &self.materials })?;

        let pre_controller: Vec<&SceneControllerBox> = self.pre_controller.iter().filter(|controller| controller.is_serializable()).collect();
        map.serialize_entry("pre_controller", &pre_controller)?;

        let post_controller: Vec<&SceneControllerBox> = self.post_controller.iter().filter(|controller| controller.is_serializable()).collect();
        map.serialize_entry("post_controller", &post_controller)?;

        map.end()
    }
}

impl<'de> Deserialize<'de> for Scene
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        struct SceneVisitor;

        impl<'de> Visitor<'de> for SceneVisitor
        {
            type Value = Scene;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result
            {
                formatter.write_str("struct Scene")
            }

            fn visit_map<V>(self, mut map: V) -> Result<Scene, V::Error>
            where V: MapAccess<'de>
            {
                let mut scene = Scene::default();

                while let Some(key) = map.next_key::<String>()?
                {
                    match key.as_str()
                    {
                        "uuid" => scene.uuid = map.next_value()?,
                        "name" => scene.name = map.next_value()?,
                        "visible" => scene.visible = map.next_value()?,
                        "main" => scene.main = map.next_value()?,
                        "data" => scene.data = map.next_value()?,
                        "nodes" =>
                        {
                            scene.nodes = map.next_value().into_iter().map(|node| Arc::new(RwLock::new(Box::new(node)))).collect()
                        }
                        "cameras" =>
                        {
                            scene.cameras = map.next_value().into_iter().collect();
                        }
                        "lights" =>
                        {
                            scene.lights = ChangeTracker::new(map.next_value().into_iter().map(|inst| RefCell::new(ChangeTracker::new(Box::new(inst)))).collect())
                        }
                        "materials" => {
                            let material_map: HashMap<u64, Box<dyn Component>> = map.next_value()?;
                            scene.materials = material_map.into_iter().map(|(id, mat)| (id, Arc::new(RwLock::new(mat)))).collect();
                        }
                        "pre_controller" =>
                        {
                            let controllers: Vec<SceneControllerBox> = map.next_value()?;
                            scene.pre_controller = controllers;
                        }
                        "post_controller" =>
                        {
                            let controllers: Vec<SceneControllerBox> = map.next_value()?;
                            scene.post_controller = controllers;
                        }
                        _ =>
                        {
                            // ignore
                            let _: serde::de::IgnoredAny = map.next_value()?;
                        }
                    }
                }

                Ok(scene)
            }
        }

        deserializer.deserialize_map(SceneVisitor)
    }
}

impl Scene
{
    pub fn new(name: &str) -> Scene
    {
        Self
        {
            id: id_manager::get_next_scene_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name.to_string(),
            visible: true,
            main: false,

            data: ChangeTracker::new(SceneData
            {
                max_lights: 10,
                environment_texture: None,
                gamma: None,
                exposure: None,
            }),

            nodes: vec![],
            cameras: vec![],
            lights: ChangeTracker::new(vec![]),
            materials: HashMap::new(),

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

    pub fn get_node_amount_recursive(&self, include_internals: bool) -> usize
    {
        let all_nodes = Scene::list_all_child_nodes(&self.nodes);

        if !include_internals
        {
            return all_nodes.iter().filter(|node| !node.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
        }

        all_nodes.len()
    }

    pub fn update(&mut self, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64)
    {
        // check moved nodes (if a node has a parent -> remove it from scene nodes)
        // this can happen when a node parent was set via set_parent
        let mut nodes_to_remove_scene = vec![];
        for node in &self.nodes
        {
            if node.read().unwrap().parent.is_some()
            {
                nodes_to_remove_scene.push(node.clone());
            }
        }

        for node_to_remove in nodes_to_remove_scene
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
                controller_item.update(self, io, frame_scale);
            }
        }

        swap(&mut pre_controller, &mut self.pre_controller);

        // update nodes
        let mut delete_nodes = vec![];
        for node in &self.nodes
        {
            let mut update_result = Node::update(node.clone(), io, time, frame_scale, frame);

            if update_result.delete_nodes.len() > 0
            {
                delete_nodes.append(&mut update_result.delete_nodes);
            }
        }

        // cameras
        let mut cameras = vec![];
        swap(&mut self.cameras, &mut cameras);
        for cam in &mut cameras
        {
            cam.update(self, io, frame_scale);
        }

        swap(&mut cameras, &mut self.cameras);

        // update post controller
        let mut post_controller = vec![];
        swap(&mut self.post_controller, &mut post_controller);
        for controller_item in &mut post_controller
        {
            if controller_item.get_base().is_enabled
            {
                controller_item.update(self, io, frame_scale);
            }
        }

        swap(&mut post_controller, &mut self.post_controller);

        // delete requested "delete_later" nodes
        for node_id in delete_nodes
        {
            self.delete_node_by_id(node_id, false, false, false, false);
        }
    }

    pub fn update_resolution(&mut self, resolution_width: u32, resolution_height: u32)
    {
        for cam in &mut self.cameras
        {
            cam.update_resolution(resolution_width, resolution_height);
            cam.init_matrices();
        }
    }

    pub fn print(&self)
    {
        console_log!(" - (SCENE) id={} name={} nodes={} cameras={} lights={} materials={}", self.id, self.name, self.nodes.len(), self.cameras.len(), self.lights.get_ref().len(), self.materials.len());

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

    pub fn add_empty_node(&mut self, name: &str, parent: Option<NodeItem>) -> NodeItem
    {
        let node = Node::new(name);

        if let Some(parent) = parent
        {
            let mut parent = parent.write().unwrap();
            parent.nodes.push(node.clone());
        }
        else
        {
            self.nodes.push(node.clone());
        }

        node
    }

    pub fn clear_nodes(&mut self)
    {
        self.nodes.clear();
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

    pub fn clear(&mut self, remove_internals: bool, delete_resources: bool)
    {
        self.cleanup_cyclic_references(None, remove_internals);

        let mut nodes_to_delete = vec![];
        for node in &self.nodes
        {
            let is_internal = node.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX);
            if !is_internal || remove_internals
            {
                nodes_to_delete.push(node.clone());
            }
        }

        for node in nodes_to_delete
        {
            let node_id = node.read().unwrap().id;
            self.delete_node_by_id(node_id, delete_resources, delete_resources, delete_resources, delete_resources);
        }

        self.lights.get_mut().retain(|light|
        {
            let is_internal = light.borrow().get_ref().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX);
            is_internal && !remove_internals
        });

        self.cameras.retain(|cam|
        {
            let is_internal = cam.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX);
            is_internal && !remove_internals
        });

        self.materials.retain(|_id, mat|
        {
            let is_internal = mat.read().unwrap().get_base().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX);
            is_internal && !remove_internals
        });

        self.pre_controller.clear();
        self.post_controller.clear();

        // re-add defaults
        self.add_defaults();
    }

    pub fn cleanup_cyclic_references(&mut self, from_node_id: Option<u64>, remove_internals: bool)
    {
        let mut node = None;
        if let Some(node_id) = from_node_id
        {
            node = self.find_node_by_id(node_id).clone();
        }

        // check camera targets and remove
        for camera in &mut self.cameras
        {
            if let Some(cam_node) = camera.node.as_ref().cloned()

            {
                if from_node_id.is_none()
                {
                    camera.remove_node();
                }

                if let Some(node_id) = from_node_id
                {
                    if cam_node.read().unwrap().id == node_id
                    {
                        camera.remove_node();
                    }
                }
            }
        }

        // controller
        for controller in &mut self.pre_controller
        {
            if let Some(node) = &node
            {
                controller.cleanup_node(node.clone());
            }
            // only cleanup everything if no node is specified
            else if from_node_id.is_none()
            {
                controller.cleanup();
            }
        }

        for controller in &mut self.post_controller
        {
            if let Some(node) = &node
            {
                controller.cleanup_node(node.clone());
            }
            // only cleanup everything if no node is specified
            else if from_node_id.is_none()
            {
                controller.cleanup();
            }
        }

        // predicate to check if a node is internal and should be removed or not
        let is_internal_predicate = Arc::new(move |node: NodeItem| -> bool
        {
            let is_internal = node.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX);
            !is_internal && !remove_internals
        });

        // clean up cyclic references in nodes
        {

            let mut all_nodes: Option<Vec<Arc<RwLock<Box<Node>>>>> = None;

            if from_node_id.is_some()
            {
                if let Some(node) = &node
                {
                    let node = node.read().unwrap();
                    all_nodes = Some(Scene::list_all_child_nodes_with_predicate(&node.nodes, is_internal_predicate.clone()));
                }
            }
            else
            {
                all_nodes = Some(Scene::list_all_child_nodes_with_predicate(&self.nodes, is_internal_predicate.clone()));
            }

            // clear instances and / clear parents
            if let Some(all_nodes) = &all_nodes
            {
                Node::cleanup_cyclic_references(&all_nodes);
            }
        }

        // remove node from all components
        if from_node_id.is_some() && self.find_node_by_id(from_node_id.unwrap()).is_some()
        {
            let node_to_remove = self.find_node_by_id(from_node_id.unwrap()).unwrap();

            let all_nodes = Self::list_all_child_nodes(&self.nodes);

            for node in all_nodes
            {
                Node::remove_node_from_components(node.clone(), node_to_remove.clone());
            }
        }
    }

    pub fn add_defaults(&mut self)
    {
        self.add_default_material();

        // post controller
        let controller = GenericController::default();
        self.post_controller.push(Box::new(controller));
    }

    pub fn clear_empty_nodes(&mut self)
    {
        Self::clear_empty_nodes_recursive(&mut self.nodes);
    }

    pub fn add_material(&mut self, material: &MaterialItem)
    {
        let id = material.read().unwrap().get_base().id;
        self.materials.insert(id, material.clone());
    }

    pub fn add_default_material(&mut self) -> MaterialItem
    {
        // check if default material already exists
        if let Some(mat) = self.get_default_material()
        {
            return mat;
        }

        let material = Material::new("default");

        let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
        material_arc.write().unwrap().get_base_mut().tags.insert_with_color_locked(ENGINE_INTERNAL_TAG, tags::DEFAULT_RED_COLOR, true);
        self.add_material(&material_arc);

        material_arc
    }

    pub fn add_empty_material(&mut self, name: &str) -> MaterialItem
    {
        let material = Material::new(name);

        let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));
        self.add_material(&material_arc);

        material_arc
    }

    pub fn get_default_material(&self) -> Option<MaterialItem>
    {
        for (_, material) in &self.materials
        {
            if material.read().unwrap().get_base().name == "default" && material.read().unwrap().get_base().tags.contains(ENGINE_INTERNAL_TAG)
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

    pub fn get_camera_by_name(&self, name: &str) -> Option<&CameraItem>
    {
        self.cameras.iter().find(|cam|{ cam.name == name })
    }

    pub fn get_camera_by_name_mut(&mut self, name: &str) -> Option<&mut CameraItem>
    {
        self.cameras.iter_mut().find(|cam|{ cam.name == name })
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

    pub fn add_empty_camera(&mut self, name: &str) -> &CameraItem
    {
        let cam = Camera::new(name.to_string());
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

    pub fn add_empty_light(&mut self, name: &str) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        self.add_light_point(name, Point3::<f32>::new(0.0, 0.0, 0.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0)
    }

    pub fn add_light_point(&mut self, name: &str, pos: Point3<f32>, color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_point(name.to_string(), pos, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_directional(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_directional(name.to_string(), pos, dir, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_spot(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, max_angle: f32, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_spot(name.to_string(), pos, dir, color, max_angle, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_hemisperical(&mut self, name: &str, dir: Vector3<f32>, color: Vector3<f32>, ground_color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_hemi(name.to_string(), dir, color, ground_color, intensity);
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

    pub fn list_all_child_nodes_with_predicate(nodes: &Vec<NodeItem>, predicate: Arc<dyn Fn(NodeItem) -> bool>) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            if !predicate(node.clone())
            {
                continue;
            }

            let child_nodes = Scene::list_all_child_nodes_with_predicate(&node.read().unwrap().nodes, predicate.clone());

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

    pub fn find_mesh_node_by_ids(&self, ids: &Vec<u64>) -> Option<NodeItem>
    {
        Node::find_mesh_node_by_ids(&self.nodes, ids)
    }

    pub fn delete_node_by_id(&mut self, id: u64, delete_mesh_resource: bool, delete_sound_sources: bool, delete_materials: bool, delete_textures: bool) -> bool
    {
        if self.find_node_by_id(id).is_none()
        {
            return false;
        }

        if delete_textures && !delete_materials
        {
            println!("WARNING: delete_textures is set to true, but delete_materials is set to false. This will not work as expected. Please set delete_materials to true.");
        }

        // ********** delete mesh resource **********
        if delete_mesh_resource
        {
            let mut mesh_resources_to_delete: HashMap<u64, MeshResourceItem> = HashMap::new();

            let mut all_nodes_to_delete;
            {
                let delete_node = self.find_node_by_id(id);
                let delete_node_arc = delete_node.unwrap();
                let delete_node = delete_node_arc.read().unwrap();

                all_nodes_to_delete = Scene::list_all_child_nodes(&delete_node.nodes);
                all_nodes_to_delete.push(delete_node_arc.clone());
            }

            for node in all_nodes_to_delete
            {
                let node = node.read().unwrap();

                for mesh in node.find_components::<Mesh>()
                {
                    component_downcast_mut!(mesh, Mesh);
                    if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                    {
                        mesh_resources_to_delete.insert(mesh_resource.read().unwrap().id, mesh_resource.clone());
                    }

                    mesh.mesh_resource = OptionOrId::None;
                }
            }

            // delete mesh resources if not in use anymore
            for (_, mesh_resource) in mesh_resources_to_delete
            {
                // used in mesh_resources_to_delete and state.mesh_resources
                if Arc::strong_count(&mesh_resource) == 2
                {
                    let mut mesh_resource = mesh_resource.write().unwrap();
                    mesh_resource.delete_later();
                }
            }
        }

        // ********** delete sound sources **********
        if delete_sound_sources
        {
            let mut sound_sources_to_delete: HashMap<u64, SoundSourceItem> = HashMap::new();

            let mut all_nodes_to_delete;
            {
                let delete_node = self.find_node_by_id(id);
                let delete_node_arc = delete_node.unwrap();
                let delete_node = delete_node_arc.read().unwrap();

                all_nodes_to_delete = Scene::list_all_child_nodes(&delete_node.nodes);
                all_nodes_to_delete.push(delete_node_arc.clone());
            }

            for node in all_nodes_to_delete
            {
                let node = node.read().unwrap();

                for sound in node.find_components::<Sound>()
                {
                    component_downcast_mut!(sound, Sound);
                    if let Some(sound_source) = sound.sound_source.as_ref()
                    {
                        sound_sources_to_delete.insert(sound_source.read().unwrap().id, sound_source.clone());
                    }

                    sound.sound_source = OptionOrId::None;
                }
            }

            // delete sound sources if not in use anymore
            for (_, sound_source) in sound_sources_to_delete
            {
                // used in mesh_resources_to_delete and state.mesh_resources
                if Arc::strong_count(&sound_source) == 2
                {
                    let mut sound_source = sound_source.write().unwrap();
                    sound_source.delete_later();
                }
            }
        }

        // ********** delete materials **********
        let mut materials_to_delete: HashMap<u64, ComponentItem> = HashMap::new();
        if delete_materials
        {
            let mut all_nodes_to_delete;
            {
                let delete_node = self.find_node_by_id(id);
                let delete_node_arc = delete_node.unwrap();
                let delete_node = delete_node_arc.read().unwrap();

                all_nodes_to_delete = Scene::list_all_child_nodes(&delete_node.nodes);
                all_nodes_to_delete.push(delete_node_arc.clone());
            }

            // find all affecting
            let mut possible_materials_to_delete: HashMap<u64, ComponentItem> = HashMap::new();
            for node in all_nodes_to_delete
            {
                let mut node_materials = vec![];

                {
                    let node = node.read().unwrap();

                    for material in node.find_components::<Material>()
                    {
                        let material_id = material.read().unwrap().id();
                        possible_materials_to_delete.insert(material_id, material.clone());
                        node_materials.push(material_id);
                    }
                }

                let mut node = node.write().unwrap();
                for material_id in node_materials
                {
                    node.remove_component_by_id(material_id);
                }
            }

            let all_nodes = Scene::list_all_child_nodes(&self.nodes);
            for (material_id, _) in &possible_materials_to_delete
            {
                let mut usage = 0;
                for node in &all_nodes
                {
                    let node = node.read().unwrap();
                    if node.find_component_by_id(*material_id).is_some()
                    {
                        usage += 1;
                    }
                }

                if usage == 0
                {
                    materials_to_delete.insert(*material_id, possible_materials_to_delete.get(material_id).unwrap().clone());
                    self.delete_material_by_id(*material_id);
                }
            }
        }

        // ********** delete textures **********
        if delete_textures
        {
            let mut all_nodes_to_delete;
            {
                let delete_node = self.find_node_by_id(id);
                let delete_node_arc = delete_node.unwrap();
                let delete_node = delete_node_arc.read().unwrap();

                all_nodes_to_delete = Scene::list_all_child_nodes(&delete_node.nodes);
                all_nodes_to_delete.push(delete_node_arc.clone());
            }

            // find all affecting
            let mut textures_to_delete: HashMap<u64, TextureItem> = HashMap::new();

            // find all textures from materials and delete them from materials
            for (_material_id, material) in &materials_to_delete
            {
                component_downcast_mut!(material, Material);
                let textures = material.get_all_textures();
                for texture in &textures
                {
                    textures_to_delete.insert(texture.read().unwrap().id, texture.clone());
                }

                material.remove_all_textures();
            }

            // delete textures if not in use anymore
            for (texture_id, texture) in textures_to_delete
            {
                let mut usage = 0;
                for (_material_id, material) in &self.materials
                {
                    component_downcast!(material, Material);
                    if material.has_texture_id(texture_id)
                    {
                        usage += 1;
                    }
                }

                if usage == 0
                {
                    console_log!("deleting texture {} {}", &texture.read().unwrap().name, texture_id);
                    texture.write().unwrap().delete_later();
                }
            }
        }

        // ********** cyclic references **********
        self.cleanup_cyclic_references(Some(id), true);

        // ********** delete directly on scene **********
        let len = self.nodes.len();
        self.nodes.retain(|node|
        {
            if node.read().unwrap().id == id
            {
                node.write().unwrap().clear_instances();
                node.write().unwrap().components.clear();
            }

            node.read().unwrap().id != id
        });

        if self.nodes.len() != len
        {
            return true;
        }

        // ********** delete on child nodes **********
        for node in &self.nodes
        {
            let deleted = node.write().unwrap().delete_child_node_by_id(id);

            if deleted
            {
                return true;
            }
        }

        false
    }

    pub fn delete_node_by_name(&mut self, name: &str, delete_mesh_resource: bool, delete_sound_sources: bool, delete_materials: bool, delete_textures: bool) -> bool
    {
        let mut node_id = None;

        {
            let node = self.find_node_by_name(name);

            if let Some(node) = node
            {
                node_id = Some(node.read().unwrap().id);
            }
        }

        if let Some(node_id) = node_id
        {
            return self.delete_node_by_id(node_id, delete_mesh_resource, delete_sound_sources, delete_materials, delete_textures);
        }

        false
    }

    pub fn multi_pick_node(&self, node: NodeItem, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Vec<ScenePickRes>
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

        self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, ignore_visible, ignore_pickable, predicate)
    }

    pub fn pick_node(&self, node: NodeItem, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Option<ScenePickRes>
    {
        let hits = self.multi_pick_node(node, ray, stop_on_first_hit, bounding_box_only, ignore_visible, ignore_pickable, predicate);

        if hits.len() > 0
        {
            return Some(hits.first().unwrap().clone());
        }

        None
    }

    pub fn pick(&self, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Option<ScenePickRes>
    {
        let nodes = Scene::list_all_child_nodes_with_mesh(&self.nodes);

        let hits = self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, ignore_visible, ignore_pickable, predicate);

        if hits.len() > 0
        {
            return Some(hits.first().unwrap().clone());
        }

        None
    }

    pub fn multi_pick(&self, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Vec<ScenePickRes>
    {
        let nodes = Scene::list_all_child_nodes_with_mesh(&self.nodes);

        self.pick_nodes(&nodes, ray, stop_on_first_hit, bounding_box_only, ignore_visible, ignore_pickable, predicate)
    }

    fn pick_nodes(&self, nodes: &Vec<Arc<RwLock<Box<Node>>>>, ray: &Ray, stop_on_first_hit: bool, bounding_box_only: bool, ignore_visible: bool, ignore_pickable: bool, predicate: Option<PickPredicate>) -> Vec<ScenePickRes>
    {
        // find hits (bbox based)
        let mut hits_bbox = vec![];

        let mut no_bbox_picking_items = vec![];

        'outer: for node_arc in nodes
        {
            let node = node_arc.read().unwrap();

            // early "return" checks
            if !ignore_visible && !node.visible
            {
                continue;
            }

            if !ignore_pickable && !node.pickable
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

            if let Some(predicate) = &predicate
            {
                if !predicate(node_arc.clone(), None)
                {
                    continue;
                }
            }

            for instance in node.instances.get_ref()
            {
                let instance_id;
                {
                    let instance = instance.read().unwrap();
                    instance_id = instance.id;
                }

                if let Some(predicate) = &predicate
                {
                    if !predicate(node_arc.clone(), Some(instance_id))
                    {
                        continue;
                    }
                }

                let instance = instance.read().unwrap();
                if !ignore_pickable && !instance.pickable
                {
                    continue;
                }

                let alpha = instance.get_cached_alpha();

                if approx_zero(alpha)
                {
                    continue;
                }

                // transformation
                let transform = instance.get_cached_world_transform();
                let transform_inverse = transform.try_inverse().unwrap();

                let ray_inverse = math::inverse_ray(ray, &transform_inverse);

                if !node.settings.pick_bbox_first
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

            let solid = !material_data.backface_culling;

            let mut joint_matrices = vec![];
            if node.skin.len() > 0
            {
                let matrices = node.get_joint_transform_vec(true);

                if let Some(matrices) = matrices
                {
                    joint_matrices = matrices;
                }
            }

            let mut morph_target_vec = vec![];
            if node.has_morph_target_weights()
            {
                let morph_targets = node.get_morph_target_weights_vec();

                if let Some(morph_targets) = morph_targets
                {
                    morph_target_vec = morph_targets;
                }
            }

            let intersection = mesh.intersect_morphed_and_skinned(ray, &ray_inverse, &transform, &transform_inverse, &joint_matrices, &morph_target_vec, solid, material_data.smooth_shading);

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
        ui.label(format!("Id: {}", self.id));
        ui.label(format!("UUID: {}", self.uuid));

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

            if ui.add(egui::DragValue::new(&mut max_lights).range(0..=20)).changed()
            {
                let data = self.get_data_mut().get_mut();

                data.max_lights = max_lights;
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Gamma:");

            if ui.add(egui::DragValue::new(&mut gamma).range(0.0..=10.0).speed(0.1)).changed()
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

            if ui.add(egui::DragValue::new(&mut exposure).range(0.0..=100.0).speed(0.1)).changed()
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