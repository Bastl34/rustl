use std::sync::{Arc, RwLock};

use crate::{console_log, helper::{self, asset_path_descriptor::AssetPathDesciptor, concurrency::execution_queue::ExecutionQueueItem, file}, resources::resources::load_binary, state::resources::texture::{Texture, TextureItem}};


pub fn load_texture_or_reuse(main_queue: ExecutionQueueItem, max_tex_res: u32, create_mipmaps: bool, path: &str, extension: Option<String>) -> anyhow::Result<TextureItem>
{
    let image_bytes = load_binary(path)?;
    let name = file::get_stem(path);

    Ok(load_texture_byte_or_reuse(main_queue, max_tex_res, create_mipmaps, &image_bytes, name.as_str(), path, extension))
}

pub fn load_texture_byte_or_reuse(main_queue: ExecutionQueueItem, max_tex_res: u32, create_mipmaps: bool, image_bytes: &Vec<u8>, name: &str, path: &str, extension: Option<String>) -> TextureItem
{
    let hash = helper::crypto::get_hash_from_byte_vec(&image_bytes);
    let hash_clone = hash.clone();
    let name_clone = name.to_string();

    let res_texture: Arc<RwLock<Option<TextureItem>>> = Arc::new(RwLock::new(None));
    let res_texture_clone = res_texture.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();

        // ***** check for reuse *****
        res = main_queue.add(Box::new(move |state|
        {
            if state.resources.textures.contains_key(&hash_clone)
            {
                console_log!("reusing texture {}", name_clone);

                *res_texture_clone.write().unwrap() = Some(state.resources.textures.get_mut(&hash_clone).unwrap().clone());
            }
        }))
    }
    res.join();

    if let Some(texture) = res_texture.read().unwrap().as_ref()
    {
        return texture.clone();
    }

    // ***** if not found -> load *****
    let mut texture = Texture::new(name, &image_bytes, extension, max_tex_res);
    texture.source = Some(AssetPathDesciptor::new_from_path(path.to_string()));
    let arc = Arc::new(RwLock::new(Box::new(texture)));

    if create_mipmaps
    {
        let mipmaps = arc.read().unwrap().create_mipmap_levels();
        arc.write().unwrap().get_data_mut().get_mut().mipmap_cache = Some(mipmaps);
        arc.write().unwrap().get_data_mut().get_mut().mipmapping = true;
    }

    // ***** add texture to state *****
    let arc_clone = arc.clone();
    let hash_clone = hash.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            state.resources.textures.insert(hash_clone.clone(), arc_clone.clone());
        }));
    }
    res.join();

    arc
}

pub fn insert_texture_or_reuse(main_queue: ExecutionQueueItem, create_mipmaps: bool, texture: Texture, name: &str) -> TextureItem
{
    let hash = texture.hash.clone();
    let hash_clone = hash.clone();
    let name_clone = name.to_string();

    let res_texture: Arc<RwLock<Option<TextureItem>>> = Arc::new(RwLock::new(None));
    let res_texture_clone = res_texture.clone();

    // ***** check for reuse *****
    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            if state.resources.textures.contains_key(&hash_clone)
            {
                console_log!("reusing texture {}", name_clone);

                *res_texture_clone.write().unwrap() = Some(state.resources.textures.get_mut(&hash_clone).unwrap().clone());
            }
        }));
    }
    res.join();

    //if let Some(texture) = res_texture.read().unwrap().as_ref()
    if let Some(texture) = res_texture.read().unwrap().as_ref()
    {
        return texture.clone();
    }

    // ***** if not found -> "load" *****
    let arc = Arc::new(RwLock::new(Box::new(texture)));

    if create_mipmaps && arc.read().unwrap().get_data().mipmap_cache.is_none()
    {
        let mipmaps = arc.read().unwrap().create_mipmap_levels();
        arc.write().unwrap().get_data_mut().get_mut().mipmap_cache = Some(mipmaps);
        arc.write().unwrap().get_data_mut().get_mut().mipmapping = true;
    }

    // ***** add to textures *****
    let arc_clone = arc.clone();
    let hash_clone = hash.clone();

    let res;
    {
        let mut main_queue = main_queue.write().unwrap();
        res = main_queue.add(Box::new(move |state|
        {
            state.resources.textures.insert(hash_clone.clone(), arc_clone.clone());
        }));
    }
    res.join();

    arc

}