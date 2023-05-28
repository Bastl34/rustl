use std::{path::Path, collections::HashMap, sync::{RwLock, Arc}};

use anyhow::Ok;

use crate::{resources::resources, helper};

use super::{manager::id_manager::IdManager, node::{NodeItem, Node}, camera::CameraItem, loader::wavefront, texture::{TextureItem, Texture}, components::material::MaterialItem, light::LightItem};

pub type SceneItem = Box<Scene>;

pub struct Scene
{
    pub id_manager: IdManager,

    pub id: u64,
    pub name: String,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<CameraItem>,
    pub lights: Vec<LightItem>,
    pub textures: HashMap<String, TextureItem>,
    pub materials: HashMap<u64, MaterialItem>,
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
            nodes: vec![],
            cameras: vec![],
            lights: vec![],
            textures: HashMap::new(),
            materials: HashMap::new(),
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

        Ok(vec![])
    }

    pub fn update(&mut self, time_delta: f32)
    {
        // update nodes
        for node in &self.nodes
        {
            node.write().unwrap().update(time_delta);
        }
    }

    pub fn print(&self)
    {
        println!(" - (SCENE) id={} name={} nodes={} cameras={} lights={} materials={} textures={}", self.id, self.name, self.nodes.len(), self.cameras.len(), self.lights.len(), self.materials.len(), self.textures.len());

        // print
        for node in &self.nodes
        {
            node.read().unwrap().print(2);
        }
    }

    pub fn add_node(&mut self, node: NodeItem)
    {
        self.nodes.push(node);
    }

    pub async fn load_texture_or_reuse(&mut self, path: &str) -> anyhow::Result<TextureItem>
    {
        let image_bytes = resources::load_binary_async(path).await?;
        let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);

        if self.textures.contains_key(&hash)
        {
            println!("reusing texture {}", path);
            return Ok(self.textures.get_mut(&hash).unwrap().clone());
        }

        let id = self.id_manager.get_next_texture_id();
        let texture = Texture::new(id, path, &image_bytes);

        let arc = Arc::new(RwLock::new(Box::new(texture)));

        self.textures.insert(hash, arc.clone());

        Ok(arc)
    }

    pub fn add_material(&mut self, id: u64, material: &MaterialItem)
    {
        self.materials.insert(id, material.clone());
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
}