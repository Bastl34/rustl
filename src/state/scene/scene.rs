use std::{path::Path, collections::HashMap, sync::{RwLock, Arc}, cell::RefCell, mem::swap};

use anyhow::Ok;
use nalgebra::Vector3;
use nalgebra::Point3;
use parry3d::{query::Ray, transformation};

use crate::{resources::resources, helper::{self, change_tracker::ChangeTracker, math::{approx_zero, self}}, state::{helper::render_item::RenderItemOption, scene::components::component::Component}, input::input_manager::InputManager, component_downcast};

use super::{manager::id_manager::IdManager, node::{NodeItem, Node}, camera::{CameraItem, Camera}, loader::wavefront, loader::gltf, texture::{TextureItem, Texture}, components::{material::{MaterialItem, Material, TextureType}, mesh::Mesh}, light::{LightItem, Light}};

pub type SceneItem = Box<Scene>;

pub struct Scene
{
    pub id_manager: IdManager,

    pub id: u64,
    pub name: String,

    pub max_lights: u32,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<CameraItem>,
    pub lights: ChangeTracker<Vec<RefCell<ChangeTracker<LightItem>>>>,
    pub textures: HashMap<String, TextureItem>,
    pub materials: HashMap<u64, MaterialItem>,

    pub render_item: RenderItemOption,
    pub lights_render_item: RenderItemOption,
}

impl Scene
{
    pub fn new(id: u64, name: &str) -> Scene
    {
        Self
        {
            id_manager: IdManager::new(),

            id: id,
            name: name.to_string(),

            max_lights: 10,

            nodes: vec![],
            cameras: vec![],
            lights: ChangeTracker::new(vec![]),
            textures: HashMap::new(),
            materials: HashMap::new(),

            render_item: None,
            lights_render_item: None,
        }
    }

    pub async fn load(&mut self, path: &str) -> anyhow::Result<Vec<u64>>
    {
        let extension = Path::new(path).extension();

        if extension.is_none()
        {
            println!("can not load {}", path);
            return Ok(vec![]);
        }
        let extension = extension.unwrap();

        if extension == "obj"
        {
            return wavefront::load(path, self).await;
        }
        else if extension == "gltf" || extension == "glb"
        {
            return gltf::load(path, self).await;
        }

        Ok(vec![])
    }

    pub fn update(&mut self, input_manager: &mut InputManager, frame_scale: f32)
    {
        // update nodes
        for node in &self.nodes
        {
            Node::update(node.clone(), input_manager, frame_scale);
        }

        let mut cameras = vec![];
        swap(&mut self.cameras, &mut cameras);
        for cam in &mut cameras
        {
            cam.update(self, input_manager, frame_scale);
        }

        swap(&mut cameras, &mut self.cameras);
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

    pub async fn load_texture_or_reuse(&mut self, path: &str, extension: Option<String>) -> anyhow::Result<TextureItem>
    {
        let image_bytes = resources::load_binary_async(path).await?;

        self.load_texture_byte_or_reuse(&image_bytes, path, extension).await
    }

    pub async fn load_texture_byte_or_reuse(&mut self, image_bytes: &Vec<u8>, name: &str, extension: Option<String>) -> anyhow::Result<TextureItem>
    {
        let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);

        if self.textures.contains_key(&hash)
        {
            println!("reusing texture {}", name);
            return Ok(self.textures.get_mut(&hash).unwrap().clone());
        }

        let id = self.id_manager.get_next_texture_id();
        let texture = Texture::new(id, name, &image_bytes, extension);

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.textures.insert(hash, arc.clone());

        Ok(arc)
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

    pub fn clear_empty_nodes(&mut self)
    {
        Self::clear_empty_nodes_recursive(&mut self.nodes);
    }

    pub async fn remove_texture(&mut self, texture: TextureItem) -> bool
    {
        let hash;
        {
            hash = texture.read().unwrap().hash.clone();
        }

        self.textures.remove(&hash).is_some()
    }

    pub fn add_material(&mut self, id: u64, material: &MaterialItem)
    {
        self.materials.insert(id, material.clone());
    }

    pub fn add_default_material(&mut self)
    {
        let material_id = self.id_manager.get_next_component_id();
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

    pub fn get_material_by_id_mut(&mut self, id: u64) -> Option<MaterialItem>
    {
        if self.materials.contains_key(&id)
        {
            let item = self.materials.get_mut(&id).unwrap();
            return Some(item.clone());
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
                if instance.borrow().find_component_by_id(id).is_some()
                {
                    instance.borrow_mut().remove_component_by_id(id);
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
        let cam = Camera::new(self.id_manager.get_next_camera_id(), name.to_string());
        self.cameras.push(Box::new(cam));

        self.cameras.last().unwrap()
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
        let light = Light::new_point(self.id_manager.get_next_light_id(), name.to_string(), pos, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_directional(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_directional(self.id_manager.get_next_light_id(), name.to_string(), pos, dir, color, intensity);
        self.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));

        self.lights.get_ref().last().unwrap()
    }

    pub fn add_light_spot(&mut self, name: &str, pos: Point3<f32>, dir: Vector3<f32>, color: Vector3<f32>, max_angle: f32, intensity: f32) -> &RefCell<ChangeTracker<Box<Light>>>
    {
        let light = Light::new_spot(self.id_manager.get_next_light_id(), name.to_string(), pos, dir, color, max_angle, intensity);
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

    fn _find_node_by_id(nodes: &Vec<NodeItem>, id: u64) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().id == id
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Scene::_find_node_by_id(&node.read().unwrap().nodes, id);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    fn _find_node_by_name(nodes: &Vec<NodeItem>, name: String) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().name == name
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Scene::_find_node_by_name(&node.read().unwrap().nodes, name.clone());
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_node_by_id(&self, id: u64) -> Option<NodeItem>
    {
        Self::_find_node_by_id(&self.nodes, id)
    }

    pub fn find_node_by_name(&self, name: &str) -> Option<NodeItem>
    {
        Self::_find_node_by_name(&self.nodes, name.to_string())
    }

    pub fn delete_node_by_id(&mut self, id: u64) -> bool
    {
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

    pub fn pick(&self, ray: &Ray, stop_on_first_hit: bool) -> Option<(f32, Vector3<f32>, NodeItem, u64, u32)>
    {
        let nodes = Scene::list_all_child_nodes_with_mesh(&self.nodes);

        // find hits (bbox based)
        let mut hits = vec![];

        for node_arc in &nodes
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

            for instance in node.instances.get_ref()
            {
                let instance = instance.borrow();

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
                let transform = instance.get_transform();
                let transform_inverse = transform.try_inverse().unwrap();

                let ray_inverse = math::inverse_ray(ray, &transform_inverse);

                let solid = true;
                let dist = mesh.intersect_b_box(&ray_inverse, solid);
                if let Some(dist) = dist
                {
                    hits.push((node_arc, instance.id, dist, transform, transform_inverse, ray_inverse));
                }
            }
        }

        if hits.len() == 0
        {
            return None;
        }

        // sort bbox dist (to get the nearest)
        hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut best_hit: Option<(f32, Vector3<f32>, NodeItem, u64, u32)> = None;

        for (node_arc, instance_id, _dist, transform, transform_inverse, ray_inverse) in hits
        {
            let node = node_arc.read().unwrap();

            let mesh = node.find_component::<Mesh>().unwrap();
            component_downcast!(mesh, Mesh);

            let material = self.get_material_or_default(node_arc.clone());
            let material = material.unwrap();
            component_downcast!(material, Material);
            let material_data = material.get_data();

            let solid = !material_data.backface_cullig;

            let intersection = mesh.intersect(ray, &ray_inverse, &transform, &transform_inverse, solid, material_data.smooth_shading);

            if let Some(intersection) = intersection
            {
                //if best_hit.is_none() || best_hit.is_some() && intersection.0 < best_hit.unwrap().0
                if best_hit.is_none()
                {
                    best_hit = Some((intersection.0, intersection.1, node_arc.clone(), instance_id, intersection.2));
                }
                else if let Some(current_best_hit) = &best_hit
                {
                    if intersection.0 < current_best_hit.0
                    {
                        best_hit = Some((intersection.0, intersection.1, node_arc.clone(), instance_id, intersection.2));
                    }
                }
            }

            //if it should return on first hit
            if best_hit.is_some() && stop_on_first_hit
            {
                return best_hit;
            }
        }

        best_hit
    }
}