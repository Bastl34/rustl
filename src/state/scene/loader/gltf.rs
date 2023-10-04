
use std::{path::Path, ffi::OsStr, sync::{Arc, RwLock}, cell::RefCell, collections::HashMap};

use gltf::{Gltf, texture};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::{Vector3, Matrix4, Point3, Point2, UnitQuaternion, Quaternion, Rotation3};

use crate::{state::scene::{scene::Scene, components::{material::{Material, MaterialItem}, mesh::Mesh, transformation::Transformation}, texture::{Texture, TextureItem, TextureAddressMode, TextureFilterMode}, light::Light, camera::Camera, node::{NodeItem, Node}}, resources::resources::load_binary_async, helper::{change_tracker::ChangeTracker, math::{approx_zero_vec3, approx_one_vec3, approx_zero}}};

pub async fn load(path: &str, scene: &mut Scene) -> anyhow::Result<Vec<u64>>
{
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

        {
            let mut tex = tex.write().unwrap();
            let tex_data = tex.get_data_mut().get_mut();

            match texture.sampler().wrap_s()
            {
                texture::WrappingMode::ClampToEdge => tex_data.address_mode_u = TextureAddressMode::ClampToEdge,
                texture::WrappingMode::MirroredRepeat => tex_data.address_mode_u = TextureAddressMode::MirrorRepeat,
                texture::WrappingMode::Repeat => tex_data.address_mode_u = TextureAddressMode::Repeat,
            }

            match texture.sampler().wrap_t()
            {
                texture::WrappingMode::ClampToEdge => tex_data.address_mode_v = TextureAddressMode::ClampToEdge,
                texture::WrappingMode::MirroredRepeat => tex_data.address_mode_v = TextureAddressMode::MirrorRepeat,
                texture::WrappingMode::Repeat => tex_data.address_mode_v = TextureAddressMode::Repeat,
            }

            if let Some(mag_filter) = texture.sampler().mag_filter()
            {
                match mag_filter
                {
                    texture::MagFilter::Nearest => tex_data.mag_filter = TextureFilterMode::Nearest,
                    texture::MagFilter::Linear => tex_data.mag_filter = TextureFilterMode::Linear,
                }
            }

            if let Some(min_filter) = texture.sampler().min_filter()
            {
                match min_filter
                {
                    texture::MinFilter::Nearest => tex_data.min_filter = TextureFilterMode::Nearest,
                    texture::MinFilter::Linear => tex_data.min_filter = TextureFilterMode::Linear,
                    texture::MinFilter::NearestMipmapNearest =>
                    {
                        tex_data.min_filter = TextureFilterMode::Nearest;
                        tex_data.mipmap_filter = TextureFilterMode::Nearest;
                    },
                    texture::MinFilter::LinearMipmapNearest =>
                    {
                        tex_data.min_filter = TextureFilterMode::Linear;
                        tex_data.mipmap_filter = TextureFilterMode::Nearest;
                    },
                    texture::MinFilter::NearestMipmapLinear =>
                    {
                        tex_data.min_filter = TextureFilterMode::Nearest;
                        tex_data.mipmap_filter = TextureFilterMode::Linear;
                    },
                    texture::MinFilter::LinearMipmapLinear =>
                    {
                        tex_data.min_filter = TextureFilterMode::Linear;
                        tex_data.mipmap_filter = TextureFilterMode::Linear;
                    },
                }
            }
        }

        loaded_textures.push((tex, texture.index()));
    }

    // because metallic and roughness are combined -> and we will use it seperatly -> the initial loaded texture should be removed again
    let mut clear_textures: Vec<TextureItem> = vec![];


    // ********** materials **********
    let mut loaded_materials: HashMap<usize, MaterialItem> = HashMap::new();
    for gltf_material in gltf.materials()
    {
        let material = load_material(&gltf_material, scene, &loaded_textures, &mut clear_textures);
        let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));

        let id;
        {
            id = material_arc.read().unwrap().id();
        }

        scene.add_material(id, &material_arc);

        let gltf_material_index = gltf_material.index().unwrap();
        loaded_materials.insert(gltf_material_index, material_arc);
    }

    // ********** scene items **********
    for gltf_scene in gltf.scenes()
    {
        for node in gltf_scene.nodes()
        {
            let ids = read_node(&node, &buffers, &loaded_materials, scene, None, &Matrix4::<f32>::identity(), 1);
            loaded_ids.extend(ids);
        }
    }

    // cleanup
    for clear_texture in clear_textures
    {
        _ = scene.remove_texture(clear_texture);
    }

    Ok(loaded_ids)
}

fn read_node(node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>, loaded_materials: &HashMap<usize, MaterialItem>, scene: &mut Scene, parent: Option<NodeItem>, parent_transform: &Matrix4<f32>, level: usize) -> Vec<u64>
{
    //let spaces = " ".repeat(level * 2);
    //println!("{}-  {} childs={}, l={}, l={}, m={}, s={}, w={}, t={:?}", spaces, node.name().unwrap(), node.children().len(), node.light().is_some(), node.camera().is_some(), node.mesh().is_some(), node.skin().is_some(), node.weights().is_none(), node.transform().matrix());

    //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/mod.rs#L69

    let mut loaded_ids: Vec<u64> = vec![];

    let local_transform = transform_to_matrix(node.transform());
    //let world_transform = parent_transform * local_transform;
    let world_transform = local_transform * parent_transform;
    let (translate, rotation, scale) = transform_decompose(node.transform());

    let mut parent_node = parent;

    // ********** lights **********
    if let Some(light) = node.light()
    {
        let light_id = scene.id_manager.get_next_light_id();
        let intensity = light.intensity();
        let color = light.color();
        let color = Vector3::<f32>::new(color[0], color[1], color[2]);

        // reference: https://github.com/flomonster/easy-gltf/blob/master/src/scene/light.rs
        let pos = Point3::<f32>::new(world_transform[(3, 0)], world_transform[(3, 1)], world_transform[(3, 2)]);
        let dir = -1.0 * Vector3::<f32>::new(world_transform[(2,0)], world_transform[(2,1)], world_transform[(2,2)]).normalize();

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

    // ********** cameras **********
    if let Some(camera) = node.camera()
    {
        let cam_id = scene.id_manager.get_next_camera_id();
        let name = camera.name().unwrap_or("Unnamed Camera");

        //https://github.com/flomonster/easy-gltf/blob/master/src/scene/camera.rs
        let pos = Point3::<f32>::new(world_transform[(3, 0)], world_transform[(3, 1)], world_transform[(3, 2)]);
        let up = Vector3::<f32>::new(world_transform[(1, 0)], world_transform[(1, 1)], world_transform[(1, 2)]);
        let forward = Vector3::<f32>::new(world_transform[(2, 0)], world_transform[(2, 1)], world_transform[(2, 2)]);
        //let right = Vector3::<f32>::new(transform[(0, 0)], transform[(0, 1)], transform[(0, 2)]);

        match camera.projection()
        {
            gltf::camera::Projection::Orthographic(ortho) =>
            {
                //TODO
            },
            gltf::camera::Projection::Perspective(pers) =>
            {
                let mut cam = Camera::new(cam_id, name.to_string());
                let cam_data = cam.get_data_mut().get_mut();
                //cam.fovy = pers.yfov().to_radians();
                cam_data.fovy = pers.yfov();
                cam_data.eye_pos = Point3::<f32>::new(pos.x, pos.y, pos.z);
                cam_data.dir = Vector3::<f32>::new(-forward.x, -forward.y, -forward.z).normalize();
                cam_data.up = Vector3::<f32>::new(up.x, up.y, up.z).normalize();
                cam_data.clipping_near = pers.znear();
                cam_data.clipping_far = pers.zfar().unwrap_or(1000.0);

                scene.cameras.push(Box::new(cam));
            },
        };
    }

    // ********** mesh **********
    if let Some(mesh) = node.mesh()
    {
        let primitives_amount = mesh.primitives().len();

        for (primitive_id, primitive) in mesh.primitives().enumerate()
        {
            let mut name = mesh.name().unwrap_or("unknown mesh").to_string();

            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let material_index = primitive.material().index();

            let mut verts: Vec<Point3::<f32>> = vec![];
            let mut uvs1: Vec<Point2<f32>> = vec![];
            let mut uvs2: Vec<Point2<f32>> = vec![];
            let mut uvs3: Vec<Point2<f32>> = vec![];
            let mut normals: Vec<Vector3<f32>> = vec![];

            let mut indices:Vec<[u32; 3]> = vec![];
            let mut uv_indices: Vec<[u32; 3]> = vec![];
            let mut normals_indices: Vec<[u32; 3]> = vec![];

            // vertices
            let gltf_vertices = reader.read_positions();
            if let Some(gltf_vertices) = gltf_vertices
            {
                for vert in gltf_vertices
                {
                    verts.push(Point3::<f32>::new(vert[0], vert[1], vert[2]));
                }
            }

            // normals
            let gltf_normals = reader.read_normals();
            if let Some(gltf_normals) = gltf_normals
            {
                for normal in gltf_normals
                {
                    normals.push(Vector3::<f32>::new(normal[0], normal[1], normal[2]));
                }
            }

            // uvs (1)
            let gltf_uvs1 = reader.read_tex_coords(0);
            if let Some(gltf_uvs1) = gltf_uvs1
            {
                for uv in gltf_uvs1.into_f32()
                {
                    // flip y coordinate
                    uvs1.push(Point2::<f32>::new(uv[0], 1.0 - uv[1]));
                }
            }

            // uvs (2)
            let gltf_uvs2 = reader.read_tex_coords(1);
            if let Some(gltf_uvs2) = gltf_uvs2
            {
                for uv in gltf_uvs2.into_f32()
                {
                    // flip y coordinate
                    uvs2.push(Point2::<f32>::new(uv[0], 1.0 - uv[1]));
                }
            }

            // uvs (3)
            let gltf_uvs3 = reader.read_tex_coords(2);
            if let Some(gltf_uvs3) = gltf_uvs3
            {
                for uv in gltf_uvs3.into_f32()
                {
                    // flip y coordinate
                    uvs3.push(Point2::<f32>::new(uv[0], 1.0 - uv[1]));
                }
            }

            // indices
            let gltf_indices: Option<Vec<u32>> = reader.read_indices().map(|indices| indices.into_u32().collect());

            if let Some(gltf_indices) = gltf_indices
            {
                for vtx in 0..gltf_indices.len() / 3
                {
                    let i0 = gltf_indices[3 * vtx];
                    let i1 = gltf_indices[3 * vtx + 1];
                    let i2 = gltf_indices[3 * vtx + 2];

                    indices.push([i0, i1, i2]);
                    uv_indices.push([i0, i1, i2]);
                    normals_indices.push([i0, i1, i2]);
                }
            }

            if verts.len() == 0 || indices.len() == 0
            {
                continue;
            }

            let mut item = Mesh::new_with_data(scene.id_manager.get_next_component_id(), "Mesh", verts, indices, uvs1, uv_indices, normals, normals_indices);
            item.get_data_mut().get_mut().uvs_2 = uvs2;
            item.get_data_mut().get_mut().uvs_3 = uvs3;

            let id = scene.id_manager.get_next_node_id();
            loaded_ids.push(id);

            if primitives_amount > 1
            {
                name = format!("{} primitive_{}", name, primitive_id);
            }

            let node_arc = Node::new(id, name.as_str());
            {
                let mut node = node_arc.write().unwrap();
                node.add_component(Arc::new(RwLock::new(Box::new(item))));

                // add material
                if let Some(material_index) = material_index
                {
                    let material_arc = loaded_materials.get(&material_index).unwrap().clone();
                    node.add_component(material_arc);
                }
                else
                {
                    let default_material = scene.get_default_material();
                    if let Some(default_material) = default_material
                    {
                        node.add_component(default_material);
                    }
                }

                // transformation
                if !approx_zero_vec3(translate) || !approx_zero_vec3(rotation) || !approx_one_vec3(scale)
                {
                    let component_id = scene.id_manager.get_next_component_id();
                    node.add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform", translate, rotation, scale)))));
                }

                // add default instance
                node.create_default_instance(node_arc.clone(), scene.id_manager.get_next_instance_id());

                // parent
                node.parent = parent_node.clone();
            }

            if parent_node.is_none()
            {
                scene.add_node(node_arc.clone());
            }
            else
            {
                Node::add_node(parent_node.clone().unwrap(), node_arc.clone());
            }

            // only if there is one primitive -> use it as parent for next childs
            if primitives_amount == 1
            {
                parent_node = Some(node_arc.clone());
            }
        }
    }

    // ********** empty transform node **********
    // if there is nothing set -> its just a transform node
    if node.camera().is_none() && node.mesh().is_none() && node.light().is_none()
    {
        // only if the node has children -> otherwise ignore it
        if node.children().len() > 0
        {
            let name = node.name().unwrap_or("transform node");
            let scene_node = Node::new(scene.id_manager.get_next_node_id(), name);

            // add transformation
            if !approx_zero_vec3(translate) || !approx_zero_vec3(rotation) || !approx_one_vec3(scale)
            {
                let component_id = scene.id_manager.get_next_component_id();
                scene_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform Test", translate, rotation, scale)))));
            }

            if parent_node.is_none()
            {
                scene.add_node(scene_node.clone());
            }
            else
            {
                Node::add_node(parent_node.clone().unwrap(), scene_node.clone());
            }

            parent_node = Some(scene_node.clone());
        }
    }

    // ********** children **********
    for child in node.children()
    {
        let ids = read_node(&child, &buffers, loaded_materials, scene, parent_node.clone(), &world_transform, level + 1);
        loaded_ids.extend(ids);
    }

    loaded_ids
}

pub fn transform_to_matrix(transform: gltf::scene::Transform) -> Matrix4<f32>
{
    let tr = transform.matrix();

    Matrix4::new
    (
        tr[0][0], tr[0][1], tr[0][2], tr[0][3],
        tr[1][0], tr[1][1], tr[1][2], tr[1][3],
        tr[2][0], tr[2][1], tr[2][2], tr[2][3],
        tr[3][0], tr[3][1], tr[3][2], tr[3][3],
    )

    //Matrix4::from_row_slice(bytemuck::cast_slice(&tr))
}

pub fn transform_decompose(transform: gltf::scene::Transform) ->(Vector3<f32>, Vector3<f32>, Vector3<f32>)
{
    let decomposed = transform.decomposed();

    let translate = Vector3::<f32>::new(decomposed.0[0], decomposed.0[1], decomposed.0[2]);
    let scale = Vector3::<f32>::new(decomposed.2[0], decomposed.2[1], decomposed.2[2]);

    let quaternion = UnitQuaternion::new_normalize
    (
        Quaternion::new
        (
            decomposed.1[3], // W
            decomposed.1[0], // X
            decomposed.1[1], // Y
            decomposed.1[2], // Z
        )
    );

    let rotation: Rotation3<f32> = quaternion.into();
    let euer_angles = rotation.euler_angles();

    let rotation = Vector3::<f32>::new(euer_angles.0, euer_angles.1, euer_angles.2);

    (translate, rotation, scale)
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

    // metallic and roughness are combined in the loaded texture
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

    // index of refraction
    if let Some(ior) = gltf_material.ior()
    {
        data.refraction_index = ior;
    }

    // unlit
    data.unlit_shading = gltf_material.unlit();

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