use std::{path::Path, collections::HashMap, sync::{RwLock, Arc}, cell::RefCell};

use anyhow::Ok;

use crate::{resources::resources, helper::{self, change_tracker::ChangeTracker}, state::helper::render_item::RenderItemOption};

use super::{manager::id_manager::IdManager, node::{NodeItem, self}, camera::CameraItem, loader::wavefront, loader::gltf, texture::{TextureItem, Texture}, components::material::{MaterialItem, Material}, light::LightItem};

pub type SceneItem = Box<Scene>;

pub struct Scene
{
    pub id_manager: IdManager,

    pub id: u64,
    pub name: String,

    pub max_lights: u32,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<RefCell<ChangeTracker<CameraItem>>>,
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

    pub fn update(&mut self, frame_scale: f32)
    {
        // update nodes
        for node in &self.nodes
        {
            node.write().unwrap().update(frame_scale);
        }
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
            cam.borrow().get_ref().print_short();
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

    fn _find_node(nodes: &Vec<NodeItem>, id: u64) -> Option<NodeItem>
    {
        for node in nodes
        {
            if node.read().unwrap().id == id
            {
                return Some(node.clone());
            }

            // check child nodes
            let result = Scene::_find_node(&node.read().unwrap().nodes, id);
            if result.is_some()
            {
                return result;
            }
        }

        None
    }

    pub fn find_node_by_id(&self, id: u64) -> Option<NodeItem>
    {
        Self::_find_node(&self.nodes, id)
    }

    pub fn delete_node_by_id(&mut self, id: u64) -> bool
    {
        let len = self.nodes.len();
        self.nodes.retain(|node|
        {
            node.read().unwrap().id != id
        });

        self.nodes.len() != len
    }
}