
use std::{path::Path, ffi::OsStr, f32::consts::E, sync::{Arc, RwLock}, cell::RefCell};

use egui::vec2;
use gltf::{Gltf, texture};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::{Vector3, Matrix4, Point3};

use crate::{state::scene::{scene::Scene, components::material::{Material, MaterialItem}, texture::{Texture, TextureItem}, light::Light}, resources::resources::load_binary_async, helper::change_tracker::ChangeTracker};

pub async fn load(path: &str, scene: &mut Scene) -> anyhow::Result<Vec<u64>>
{
    dbg!(path);

    let gltf_content = load_binary_async(path).await?;

    let mut gltf = Gltf::from_slice(gltf_content.as_slice())?;
    let mut blob = gltf.blob.take();

    let mut loaded_ids: Vec<u64> = vec![];

    // ********** buffers **********
    let mut buffers: Vec<gltf::buffer::Data> = vec![];

    for buffer in gltf.buffers()
    {
        let data = load_buffer(path, &mut blob, &buffer).await;
        buffers.push(gltf::buffer::Data(data));
    }

    // ********** textures **********
    let mut loaded_textures = vec![];

    for texture in gltf.textures()
    {
        let (bytes, extension) = load_texture(path, &texture, &buffers).await;

        let tex = scene.load_texture_byte_or_reuse(&bytes, texture.name().unwrap_or("unknown"), extension).await?;

        loaded_textures.push((tex, texture.index()));
    }

    // because metallic and roughness are combined -> and we will use it seperatly -> the initial loaded texture should be removed again
    let mut clear_textures: Vec<TextureItem> = vec![];


    // ********** materials **********
    let mut loaded_materials = vec![];
    for gltf_material in gltf.materials()
    {
        let material = load_material(&gltf_material, scene, &loaded_textures, &mut clear_textures);
        let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));

        let id;
        {
            id = material_arc.read().unwrap().id();
        }

        scene.add_material(id, &material_arc);

        loaded_materials.push((material_arc, gltf_material.index()));
    }

    // ********** scene items **********
    for gltf_scene in gltf.scenes()
    {
        for node in gltf_scene.nodes()
        {
            read_node(&node, scene, &Matrix4::<f32>::identity());
        }
    }


    /*
    if let Some(lights) = gltf.lights()
    {
        for light in lights
        {
            let light_id = scene.id_manager.get_next_light_id();
            let intensity = light.intensity();
            let name = light.name().unwrap_or("unknown");
            let color = light.color();
            let color = Vector3::<f32>::new(color[0], color[1], color[2]);

            let range = light.range();
            light.

            match light.kind()
            {
                gltf::khr_lights_punctual::Kind::Directional =>
                {
                },
                gltf::khr_lights_punctual::Kind::Point =>
                {
                    let light = Light::new_point(light_id, Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                    scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                },
                gltf::khr_lights_punctual::Kind::Spot { inner_cone_angle, outer_cone_angle } =>
                {

                },
            };
        }
    }
    */


    // cleanup
    for clear_texture in clear_textures
    {
        _ = scene.remove_texture(clear_texture);
    }

    Ok(loaded_ids)
}

fn read_node(node: &gltf::Node, scene: &mut Scene, parent_transform: &Matrix4<f32>)
{
    //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/mod.rs#L69


    let local_transform = transform_to_matrix(node.transform());
    let transform = parent_transform * local_transform;

    // ********** lights **********
    if let Some(light) = node.light()
    {
        let light_id = scene.id_manager.get_next_light_id();
        let intensity = light.intensity();
        let color = light.color();
        let color = Vector3::<f32>::new(color[0], color[1], color[2]);

        // reference: https://github.com/flomonster/easy-gltf/blob/master/src/scene/light.rs
        let pos = Point3::<f32>::new(transform[(3, 0)], transform[(3, 1)], transform[(3, 2)]);
        let dir = -1.0 * Vector3::<f32>::new(transform[(2,0)], transform[(2,1)], transform[(2,2)]).normalize();

        // let range = light.range(); TODO

        match light.kind()
        {
            gltf::khr_lights_punctual::Kind::Directional =>
            {
                let name = light.name().unwrap_or("Directional");
                let light = Light::new_directional(light_id, name.to_string(), pos, dir, color, intensity);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            },
            gltf::khr_lights_punctual::Kind::Point =>
            {
                let name = light.name().unwrap_or("Point");
                let light = Light::new_point(light_id, name.to_string(), pos, color, intensity);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            },
            gltf::khr_lights_punctual::Kind::Spot { inner_cone_angle: _, outer_cone_angle } =>
            {
                let name = light.name().unwrap_or("Point");
                let light = Light::new_spot(light_id, name.to_string(), pos, dir, color, outer_cone_angle, intensity);
                scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
            },
        };
    }
}

pub fn transform_to_matrix(transform: gltf::scene::Transform) -> Matrix4<f32>
{
    // TODO: validate
    let tr = transform.matrix();
    Matrix4::new
    (
        tr[0][0], tr[0][1], tr[0][2], tr[0][3],
        tr[1][0], tr[1][1], tr[1][2], tr[1][3],
        tr[2][0], tr[2][1], tr[2][2], tr[2][3],
        tr[3][0], tr[3][1], tr[3][2], tr[3][3],
    )
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

pub fn get_normal_texture_by_index(texture_info: &gltf::material::NormalTexture<'_>, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>) -> Option<Arc<RwLock<Box<Texture>>>>
{
    let index = texture_info.texture().index();
    let tex_index = loaded_textures.iter().position(|t| t.1 == index);
    if let Some(tex_index) = tex_index
    {
        return Some(loaded_textures.get(tex_index).unwrap().0.clone());
    }

    None
}

pub fn get_ao_texture_by_index(texture_info: &gltf::material::OcclusionTexture<'_>, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>) -> Option<Arc<RwLock<Box<Texture>>>>
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

pub fn load_material(gltf_material: &gltf::Material<'_>, scene: &mut Scene, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>, clear_textures: &mut Vec<TextureItem>) -> Material
{
    let component_id = scene.id_manager.get_next_component_id();
    let mut material = Material::new(component_id, gltf_material.name().unwrap_or("unknown"));
    let data = material.get_data_mut().get_mut();

    let base_color = gltf_material.pbr_metallic_roughness().base_color_factor();
    data.base_color = Vector3::<f32>::new(base_color[0], base_color[1], base_color[2]);
    data.alpha = base_color[3];

    // base/albedo texture
    if let Some(tex) = gltf_material.pbr_metallic_roughness().base_color_texture()
    {
        if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
        {
            data.texture_base = Some(texture);
        }
    }

    // normal
    if let Some(tex) = gltf_material.normal_texture()
    {
        if let Some(texture) = get_normal_texture_by_index(&tex, &loaded_textures)
        {
            data.texture_normal = Some(texture);
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

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // roughness
    data.roughness = gltf_material.pbr_metallic_roughness().roughness_factor();

    if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
    {
        if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
        {
            let tex = texture.read().unwrap();
            let name = format!("{} roughness", tex.name);
            let roughness_tex = Texture::new_from_image_channel(scene.id_manager.get_next_texture_id(), name.as_str(), &tex, 1);
            let tex_arc = scene.insert_texture_or_reuse(roughness_tex, name.as_str());
            data.texture_reflectivity = Some(tex_arc);

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // emissive / ambient
    let emissive = gltf_material.emissive_factor();
    data.ambient_color = Vector3::<f32>::new(emissive[0], emissive[1], emissive[2]);

    if let Some(tex) = gltf_material.emissive_texture()
    {
        if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
        {
            data.texture_ambient = Some(texture);
        }
    }

    // ambient occlusion
    if let Some(tex) = gltf_material.occlusion_texture()
    {
        if let Some(texture) = get_ao_texture_by_index(&tex, &loaded_textures)
        {
            data.texture_ambient_occlusion = Some(texture);
        }
    }

    // backface culling
    data.backface_cullig = !gltf_material.double_sided();

    material
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