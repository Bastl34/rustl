use std::{path::Path, collections::HashMap, sync::{RwLock, Arc}};

use anyhow::Ok;

use crate::{resources::resources, helper};

use super::{manager::id_manager::IdManager, node::NodeItem, camera::CameraItem, loader::wavefront, texture::{TextureItem, Texture}};

pub type SceneItem = Box<Scene>;

pub struct Scene
{
    pub id_manager: IdManager,

    pub id: u32,
    pub name: String,

    pub nodes: Vec<NodeItem>,
    pub cameras: Vec<CameraItem>,
    pub textures: HashMap<String, TextureItem>,
}

impl Scene
{
    pub fn new(id: u32, name: &str) -> Scene
    {
        Self
        {
            id_manager: IdManager::new(),
            id: id,
            name: name.to_string(),
            nodes: vec![],
            cameras: vec![],
            textures: HashMap::new(),
        }
    }

    pub async fn load(&mut self, path: &str) -> anyhow::Result<Vec<u32>>
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

    pub async fn load_texture(&mut self, path: &str) -> anyhow::Result<TextureItem>
    {
        let image_bytes = resources::load_binary_async(path).await?;
        let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);

        if self.textures.contains_key(&hash)
        {
            return Ok(self.textures.get_mut(&hash).unwrap().clone());
        }

        let id = self.id_manager.get_next_texture_id();
        let texture = Texture::new(id, path, &image_bytes);

        Ok(Arc::new(RwLock::new(Box::new(texture))))
    }
}