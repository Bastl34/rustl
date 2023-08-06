
use std::{path::Path, ffi::OsStr, f32::consts::E, sync::{Arc, RwLock}};

use egui::vec2;
use gltf::{Gltf, texture};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::Vector3;

use crate::{state::scene::{scene::Scene, components::material::Material, texture::{Texture, TextureItem}}, resources::resources::load_binary_async, new_shared_component};

pub async fn load(path: &str, scene: &mut Scene) -> anyhow::Result<Vec<u64>>
{
    dbg!(path);

    let gltf_content = load_binary_async(path).await?;

    let mut gltf = Gltf::from_slice(gltf_content.as_slice())?;
    let mut blob = gltf.blob.take();

    let mut loaded_ids: Vec<u64> = vec![];

    let mut buffers: Vec<gltf::buffer::Data> = vec![];

    for buffer in gltf.buffers()
    {
        let data = load_buffer(path, &mut blob, &buffer).await;
        buffers.push(gltf::buffer::Data(data));
    }

    let mut loaded_textures = vec![];

    for texture in gltf.textures()
    {
        let (bytes, extension) = load_texture(path, &texture, &buffers).await;

        let tex = scene.load_texture_byte_or_reuse(&bytes, texture.name().unwrap_or("unknown"), extension).await?;

        loaded_textures.push((tex, texture.index()));
    }

    // because metallic and roughness are combined -> and we will use it seperatly -> the initial loaded texture should be removed again
    let clear_textures: Vec<TextureItem> = vec![];

    for gltf_material in gltf.materials()
    {
        let component_id = scene.id_manager.get_next_component_id();
        let mut material = Material::new(component_id, gltf_material.name().unwrap_or("unknown"));
        let data = material.get_data_mut().get_mut();

        let base_color = gltf_material.pbr_metallic_roughness().base_color_factor();
        data.base_color = Vector3::<f32>::new(base_color[0], base_color[1], base_color[2]);
        data.alpha = base_color[3];

        // base/albedo texture
        if let Some(base_tex) = gltf_material.pbr_metallic_roughness().base_color_texture()
        {
            if let Some(texture) = get_texture_by_index(&base_tex, &loaded_textures)
            {
                data.texture_base = Some(texture);
            }
        }

        // specular
        let specular = gltf_material.specular();
        if let Some(specular) = specular
        {
            // https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_specular/README.md
            let specular_color = specular.specular_color_factor();
            let specular_color_factor = specular.specular_factor();

            data.specular_color = Vector3::<f32>::new(specular_color[0] * specular_color_factor, specular_color[1] * specular_color_factor, specular_color[2] * specular_color_factor);

            if let Some(specular_tex) = specular.specular_color_texture()
            {
                if let Some(texture) = get_texture_by_index(&specular_tex, &loaded_textures)
                {
                    data.texture_specular = Some(texture);
                }
            }
        }
        else
        {
            // if there is no specular color -> use base color
            data.specular_color = data.base_color * 0.8;
        }

        // reflectivity

        // do not use full metallic_factor as reflectivity --> otherwise the object will be just complete mirror if metallic is set to 1.0
        data.reflectivity = gltf_material.pbr_metallic_roughness().metallic_factor() * 0.5;

        //TODO: metallic and roughness are combined in the loaded texture
        //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/model/material/pbr.rs#L22

        if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
        {
            if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
            {
                let tex = texture.read().unwrap();
                let name = format!("{} metallic", tex.name);
                let roughness_tex = Texture::new_from_image_channel(scene.id_manager.get_next_texture_id(), name.as_str(), &tex, 2);
                let tex_arc = scene.insert_texture_or_reuse(roughness_tex, name.as_str());
                data.texture_reflectivity = Some(tex_arc);
            }
        }

        // roughness
    }

    for scene in gltf.scenes()
    {
        for node in scene.nodes()
        {
            /*
            println!(
                "Node #{} has {} children",
                node.index(),
                node.children().count(),
            );
            */
        }
    }

    // cleanup
    for clear_texture in clear_textures
    {
        _ = scene.remove_texture(clear_texture);
    }

    Ok(loaded_ids)
}


pub fn get_texture_by_index(texture_info: &texture::Info<'_>, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>) -> Option<Arc<RwLock<Box<Texture>>>>
{
    let index = texture_info.texture().index();
    let tex_index = loaded_textures.iter().position(|t| t.1 == index);
    if let Some(tex_index) = tex_index
    {
        return Some(loaded_textures.get(tex_index).unwrap().0.clone());
    }

    None
}

pub fn get_path(item_path: &String, gltf_path: &str) -> String
{
    let mut item_path = item_path.clone();

    if Path::new(&item_path).is_relative()
    {
        let parent = Path::new(gltf_path).parent();
        if let Some(parent) = parent
        {
            item_path = parent.join(item_path).to_str().unwrap().to_string();
        }
    }

    item_path.replace("\\", "/")
}

pub async fn load_buffer(gltf_path: &str, blob: &mut Option<Vec<u8>>, buffer: &gltf::Buffer<'_>) -> Vec<u8>
{
    let mut data = match buffer.source()
    {
        gltf::buffer::Source::Bin =>
        {
            blob.take().unwrap()
        },
        gltf::buffer::Source::Uri(uri) =>
        {
            if uri.starts_with("data:")
            {
                let encoded = uri.split(',').nth(1).unwrap();
                STANDARD.decode(encoded).unwrap()
            }
            else
            {
                let buffer_path = get_path(&uri.to_string(), gltf_path);
                load_binary_async(buffer_path.as_str()).await.unwrap()
            }
        },
    };

    // padding
    while data.len() % 4 != 0
    {
        data.push(0);
    }

    data
}

// inpired from here: https://github.com/flomonster/easy-gltf/blob/master/src/utils/gltf_data.rs
pub async fn load_texture(gltf_path: &str, texture: &gltf::Texture<'_>, buffers: &Vec<gltf::buffer::Data>) -> (Vec<u8>, Option<String>)
{
    let image = texture.source();

    match image.source()
    {
        gltf::image::Source::View { view, mime_type } =>
        {
            let parent_buffer_data = &buffers[view.buffer().index()].0;
            let data = &parent_buffer_data[view.offset()..view.offset() + view.length()];
            let mime_type = mime_type.replace('/', ".");
            let extension = Path::new(&mime_type).extension().and_then(OsStr::to_str);
            dbg!(extension);

            (data.to_vec(), extension.map(str::to_string))
        }
        gltf::image::Source::Uri { uri, mime_type } =>
        {
            if uri.starts_with("data:")
            {
                let encoded = uri.split(',').nth(1).unwrap();
                //let data = URL_SAFE_NO_PAD.decode(encoded).unwrap();
                let data = STANDARD.decode(encoded).unwrap();
                let mime_type = if let Some(ty) = mime_type
                {
                    ty
                }
                else
                {
                    uri.split(',').next().unwrap().split(':').nth(1).unwrap().split(';').next().unwrap()
                };
                let mime_type = mime_type.replace('/', ".");
                let extension = Path::new(&mime_type).extension().and_then(OsStr::to_str);

                dbg!(extension);

                (data, extension.map(str::to_string))
            }
            else
            {
                let item_path = get_path(&uri.to_string(), gltf_path);
                let bytes = load_binary_async(item_path.as_str()).await.unwrap();

                let extension;
                if let Some(mime_type) = mime_type
                {
                    let mime_type = mime_type.replace('/', ".");
                    extension = Path::new(&mime_type).extension().and_then(OsStr::to_str);
                    (bytes, extension.map(str::to_string))
                }
                else
                {
                    (bytes, None)
                }
            }
        }
    }
}