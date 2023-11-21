
use std::{path::Path, ffi::OsStr, sync::{Arc, RwLock, Weak}, cell::RefCell, collections::HashMap, mem::swap};

use gltf::{Gltf, texture};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::{Vector3, Matrix4, Point3, Point2, UnitQuaternion, Quaternion, Rotation3};

use crate::{state::scene::{scene::Scene, components::{material::{Material, MaterialItem, TextureState, TextureType}, mesh::Mesh, transformation::Transformation, component::Component}, texture::{Texture, TextureItem, TextureAddressMode, TextureFilterMode}, light::Light, camera::Camera, node::{NodeItem, Node}, utilities::scene_utils::{load_texture_byte_or_reuse, execute_on_scene_mut_and_wait, insert_texture_or_reuse, get_new_tex_id, get_new_component_id, get_new_light_id, get_new_camera_id, get_new_node_id, get_new_instance_id}}, resources::resources::load_binary, helper::{change_tracker::ChangeTracker, math::{approx_zero_vec3, approx_one_vec3}, file::get_stem, concurrency::execution_queue::ExecutionQueueItem}, rendering::{scene, light}};

pub fn load(path: &str, scene_id: u64, main_queue: ExecutionQueueItem, create_root_node: bool, reuse_materials: bool, object_only: bool, create_mipmaps: bool) -> anyhow::Result<Vec<u64>>
{
    let gltf_content = load_binary(path)?;

    let mut gltf = Gltf::from_slice(gltf_content.as_slice())?;
    let mut blob = gltf.blob.take();

    let mut loaded_ids: Vec<u64> = vec![];

    // ********** buffers **********
    let mut buffers: Vec<gltf::buffer::Data> = vec![];

    for buffer in gltf.buffers()
    {
        let data = load_buffer(path, &mut blob, &buffer);
        buffers.push(gltf::buffer::Data(data));
    }

    // ********** textures **********
    dbg!("loading textures...");
    let mut loaded_textures = vec![];

    for gltf_texture in gltf.textures()
    {
        let (bytes, extension) = load_texture(path, &gltf_texture, &buffers);

        let tex = load_texture_byte_or_reuse(scene_id, main_queue.clone(), &bytes, gltf_texture.name().unwrap_or("unknown"), extension);
        apply_texture_filtering_settings(tex.clone(), &gltf_texture, create_mipmaps);

        loaded_textures.push((tex, gltf_texture.index()));
    }

    // because metallic and roughness are combined -> and we will use it seperatly -> the initial loaded texture should be removed again
    let mut clear_textures: Vec<TextureItem> = vec![];


    // ********** materials **********
    dbg!("loading materials...");
    let resource_name = get_stem(path);
    let mut loaded_materials: HashMap<usize, MaterialItem> = HashMap::new();
    for gltf_material in gltf.materials()
    {
        let gltf_material_index = gltf_material.index().unwrap();

        let material: Arc<RwLock<Option<MaterialItem>>> = Arc::new(RwLock::new(None));
        let material_clone = material.clone();

        if reuse_materials
        {
            if let Some(name) = gltf_material.name()
            {
                let name = name.to_string();
                execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                {
                    *material_clone.write().unwrap() = scene.get_material_by_name(name.as_str());
                }));
            }
        }

        let material = material.read().unwrap().clone();
        if let Some(material) = material
        {
            loaded_materials.insert(gltf_material_index, material.clone());
        }
        else
        {
            let material = load_material(&gltf_material, scene_id, main_queue.clone(), &loaded_textures, &mut clear_textures, create_mipmaps, resource_name.clone().clone());
            let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));

            let id;
            {
                id = material_arc.read().unwrap().id();
            }

            let material_arc_clone = material_arc.clone();
            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
            {
                scene.add_material(id, &material_arc_clone);
            }));

            loaded_materials.insert(gltf_material_index, material_arc);
        }
    }

    // ********** scene items **********
    dbg!("loading scene items...");
    let mut root_node = None;
    if create_root_node
    {
        let node_id = get_new_node_id(main_queue.clone(), scene_id);
        loaded_ids.push(node_id);

        let node = Node::new(node_id, resource_name.as_str());
        node.write().unwrap().root_node = true;
        root_node = Some(node.clone());
    }

    dbg!("------");
    dbg!(path);
    dbg!(create_root_node);

    dbg!("reading nodes...");
    let mut scene_nodes = vec![];
    for gltf_scene in gltf.scenes()
    {
        for node in gltf_scene.nodes()
        {
            let nodes = read_node(&node, &buffers, object_only, &loaded_materials, scene_id, main_queue.clone(), root_node.clone(), &Matrix4::<f32>::identity(), 1);
            scene_nodes.extend(nodes.clone());

            let all_nodes = Scene::list_all_child_nodes(&nodes);

            for node in all_nodes
            {
                loaded_ids.push(node.read().unwrap().id);
            }

        }
    }

    // ********** add to scene **********
    dbg!("adding nodes to scene...");
    if let Some(root_node) = root_node
    {
        execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
        {
            scene.add_node(root_node.clone());
        }));
    }

    if scene_nodes.len() > 0
    {
        execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
        {
            for scene_node in &scene_nodes
            {
                scene.add_node(scene_node.clone());
            }
        }));
    }

    // cleanup
    dbg!("cleanup...");
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        for clear_texture in &clear_textures
        {
            scene.delete_texture_by_id(clear_texture.read().unwrap().id);
        }
    }));

    Ok(loaded_ids)
}

fn read_node(node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>, object_only: bool, loaded_materials: &HashMap<usize, MaterialItem>, scene_id: u64, main_queue: ExecutionQueueItem, parent: Option<NodeItem>, parent_transform: &Matrix4<f32>, level: usize) -> Vec<Arc<RwLock<Box<Node>>>>
{
    //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/mod.rs#L69

    //let mut loaded_ids: Vec<u64> = vec![];
    let mut scene_nodes = vec![];

    let local_transform = transform_to_matrix(node.transform());
    //let world_transform = parent_transform * local_transform;
    let world_transform = local_transform * parent_transform;
    let (translate, rotation, scale) = transform_decompose(node.transform());

    let mut parent_node = parent;

    // ********** lights **********
    if !object_only
    {
        if let Some(light) = node.light()
        {
            let light_id = get_new_light_id(main_queue.clone(), scene_id);
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
                    let name = light.name().unwrap_or("Directional").to_string();
                    println!("load light {}", name.as_str());
                    let name = Arc::new(name);

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let light = Light::new_directional(light_id, (*name).clone(), pos, dir, color, intensity);
                        scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                    }));
                },
                gltf::khr_lights_punctual::Kind::Point =>
                {
                    let name = light.name().unwrap_or("Point").to_string();
                    println!("load light {}", name.as_str());
                    let name = Arc::new(name);

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let light = Light::new_point(light_id, (*name).clone(), pos, color, intensity);
                        scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                    }));
                },
                gltf::khr_lights_punctual::Kind::Spot { inner_cone_angle: _, outer_cone_angle } =>
                {
                    let name = light.name().unwrap_or("Point").to_string();
                    println!("load light {}", name.as_str());
                    let name = Arc::new(name);

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let light = Light::new_spot(light_id, (*name).clone(), pos, dir, color, outer_cone_angle, intensity);
                        scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                    }));
                },
            };
        }
    }

    // ********** cameras **********
    if !object_only
    {
        if let Some(camera) = node.camera()
        {
            let cam_id = get_new_camera_id(main_queue.clone(), scene_id);
            let name = camera.name().unwrap_or("Unnamed Camera").to_string();
            let name = Arc::new(name);

            println!("load camera {}", name.as_str());

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
                    let yfov = pers.yfov();
                    let znear = pers.znear();
                    let zfar = pers.zfar();

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let mut cam = Camera::new(cam_id, (*name).clone());
                        let cam_data = cam.get_data_mut().get_mut();
                        //cam.fovy = yfov.to_radians();
                        cam_data.fovy = yfov;
                        cam_data.eye_pos = Point3::<f32>::new(pos.x, pos.y, pos.z);
                        cam_data.dir = Vector3::<f32>::new(-forward.x, -forward.y, -forward.z).normalize();
                        cam_data.up = Vector3::<f32>::new(up.x, up.y, up.z).normalize();
                        cam_data.clipping_near = znear;
                        cam_data.clipping_far = zfar.unwrap_or(1000.0);

                        scene.cameras.push(Box::new(cam));
                    }));
                },
            };
        }
    }

    // ********** mesh **********
    if let Some(mesh) = node.mesh()
    {
        let primitives_amount = mesh.primitives().len();

        for (primitive_id, primitive) in mesh.primitives().enumerate()
        {
            let mut name = mesh.name().unwrap_or("unknown mesh").to_string();

            println!("load mesh {}", name.as_str());

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

            let component_id = get_new_component_id(main_queue.clone(), scene_id);
            let mut item = Mesh::new_with_data(component_id, "Mesh", verts, indices, uvs1, uv_indices, normals, normals_indices);
            item.get_data_mut().get_mut().uvs_2 = uvs2;
            item.get_data_mut().get_mut().uvs_3 = uvs3;

            let id = get_new_node_id(main_queue.clone(), scene_id);
            //loaded_ids.push(id);

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
                /*
                else
                {
                    let default_material = scene.get_default_material();
                    if let Some(default_material) = default_material
                    {
                        node.add_component(default_material);
                    }
                }
                */

                // transformation
                if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
                {
                    let component_id = get_new_component_id(main_queue.clone(), scene_id);
                    node.add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform", translate, rotation, scale)))));
                }

                // add default instance
                let instance_id = get_new_instance_id(main_queue.clone(), scene_id);
                node.create_default_instance(node_arc.clone(), instance_id);

                // parent
                node.parent = parent_node.clone();
            }

            if parent_node.is_none()
            {
                scene_nodes.push(node_arc.clone());
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
            println!("load empty {}", name);

            let node_id = get_new_node_id(main_queue.clone(), scene_id);
            let scene_node = Node::new(node_id, name);

            // add transformation
            if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
            {
                let component_id = get_new_component_id(main_queue.clone(), scene_id);
                scene_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform", translate, rotation, scale)))));
            }

            if parent_node.is_none()
            {
                scene_nodes.push(scene_node.clone());
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
        let loaded_nodes = read_node(&child, &buffers, object_only, loaded_materials, scene_id, main_queue.clone(), parent_node.clone(), &world_transform, level + 1);
        scene_nodes.extend(loaded_nodes);
    }

    scene_nodes
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

fn apply_texture_filtering_settings<'a>(tex: Arc<RwLock<Box<Texture>>>, gltf_texture: &gltf::Texture<'a>, create_mipmaps: bool)
{
    let mut tex = tex.write().unwrap();
    let tex_data = tex.get_data_mut().get_mut();
    tex_data.mipmapping = create_mipmaps;

    match gltf_texture.sampler().wrap_s()
    {
        texture::WrappingMode::ClampToEdge => tex_data.address_mode_u = TextureAddressMode::ClampToEdge,
        texture::WrappingMode::MirroredRepeat => tex_data.address_mode_u = TextureAddressMode::MirrorRepeat,
        texture::WrappingMode::Repeat => tex_data.address_mode_u = TextureAddressMode::Repeat,
    }

    match gltf_texture.sampler().wrap_t()
    {
        texture::WrappingMode::ClampToEdge => tex_data.address_mode_v = TextureAddressMode::ClampToEdge,
        texture::WrappingMode::MirroredRepeat => tex_data.address_mode_v = TextureAddressMode::MirrorRepeat,
        texture::WrappingMode::Repeat => tex_data.address_mode_v = TextureAddressMode::Repeat,
    }

    if let Some(mag_filter) = gltf_texture.sampler().mag_filter()
    {
        match mag_filter
        {
            texture::MagFilter::Nearest => tex_data.mag_filter = TextureFilterMode::Nearest,
            texture::MagFilter::Linear => tex_data.mag_filter = TextureFilterMode::Linear,
        }
    }

    if let Some(min_filter) = gltf_texture.sampler().min_filter()
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

pub fn load_material(gltf_material: &gltf::Material<'_>, scene_id: u64, main_queue: ExecutionQueueItem, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>, clear_textures: &mut Vec<TextureItem>, create_mipmaps: bool, resource_name: String) -> Material
{
    //let component_id = scene.id_manager.get_next_component_id();
    let component_id: u64 = get_new_component_id(main_queue.clone(), scene_id);

    let mut material = Material::new(component_id, gltf_material.name().unwrap_or("unknown"));
    let material_name = material.get_base().name.clone();
    let data = material.get_data_mut().get_mut();

    let base_color = gltf_material.pbr_metallic_roughness().base_color_factor();
    data.base_color = Vector3::<f32>::new(base_color[0], base_color[1], base_color[2]);
    data.alpha = base_color[3];

    // base/albedo texture
    if let Some(tex) = gltf_material.pbr_metallic_roughness().base_color_texture()
    {
        if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
        {
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Base);
            data.texture_base = Some(TextureState::new(texture));
        }
    }

    // normal
    if let Some(tex) = gltf_material.normal_texture()
    {
        if let Some(texture) = get_normal_texture_by_index(&tex, &loaded_textures)
        {
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Normal);
            data.texture_normal = Some(TextureState::new(texture));
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
                set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Specular);
                data.texture_specular = Some(TextureState::new(texture));
            }
        }
    }
    else
    {
        // if there is no specular color -> use base color
        data.specular_color = data.base_color * 0.8;
    }

    // reflectivity (metallic and roughness are combined in the loaded texture)
    // do not use full metallic_factor as reflectivity --> otherwise the object will be just complete mirror if metallic is set to 1.0
    //data.reflectivity = gltf_material.pbr_metallic_roughness().metallic_factor() * 0.5; // TODO CHECK ME
    data.reflectivity = gltf_material.pbr_metallic_roughness().metallic_factor();

    if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
    {
        if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
        {
            let tex_id: u64 = get_new_tex_id(main_queue.clone(), scene_id);

            let reflectivity_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                reflectivity_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 2);
            }
            let tex_arc: Arc<RwLock<Box<Texture>>> = insert_texture_or_reuse(scene_id, main_queue.clone(), reflectivity_tex, tex_name.as_str());

            apply_texture_filtering_settings(tex_arc.clone(), &metallic_roughness_tex.texture(), create_mipmaps);
            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::Reflectivity);
            data.texture_reflectivity = Some(TextureState::new(tex_arc));

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // roughness (metallic and roughness are combined in the loaded texture)
    data.roughness = gltf_material.pbr_metallic_roughness().roughness_factor();

    if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
    {
        if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
        {
            let tex_id: u64 = get_new_tex_id(main_queue.clone(), scene_id);

            let roughness_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                roughness_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 1);
            }
            let tex_arc = insert_texture_or_reuse(scene_id, main_queue.clone(), roughness_tex, tex_name.as_str());

            apply_texture_filtering_settings(tex_arc.clone(), &metallic_roughness_tex.texture(), create_mipmaps);
            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::Roughness);
            data.texture_roughness = Some(TextureState::new(tex_arc));

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
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::AmbientEmissive);
            data.texture_ambient = Some(TextureState::new(texture));
        }
    }

    // ambient occlusion
    if let Some(ao_gltf_tex) = gltf_material.occlusion_texture()
    {
        if let Some(texture) = get_ao_texture_by_index(&ao_gltf_tex, &loaded_textures)
        {
            let tex_id: u64 = get_new_tex_id(main_queue.clone(), scene_id);

            //data.texture_ambient_occlusion = Some(TextureState::new(texture));
            let ao_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                ao_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 0);
            }
            let tex_arc: Arc<RwLock<Box<Texture>>> = insert_texture_or_reuse(scene_id, main_queue.clone(), ao_tex, tex_name.as_str());

            apply_texture_filtering_settings(tex_arc.clone(), &ao_gltf_tex.texture(), create_mipmaps);
            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::AmbientOcclusion);
            data.texture_ambient_occlusion = Some(TextureState::new(tex_arc));

            // add texture to clearable textures
            clear_textures.push(texture.clone());
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

fn set_texture_name(texture: Arc<RwLock<Box<Texture>>>, material_name: String, resource_name: String, texture_type: TextureType)
{
    let mut texture = texture.write().unwrap();

    if texture.name == "unknown"
    {
        if material_name == "unknown"
        {
            texture.name = resource_name;
        }
        else
        {
            texture.name = material_name;
        }

        texture.name = format!("{} {}", texture.name, texture_type.to_string());
    }
}

pub fn load_buffer(gltf_path: &str, blob: &mut Option<Vec<u8>>, buffer: &gltf::Buffer<'_>) -> Vec<u8>
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
                load_binary(buffer_path.as_str()).unwrap()
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
pub fn load_texture(gltf_path: &str, texture: &gltf::Texture<'_>, buffers: &Vec<gltf::buffer::Data>) -> (Vec<u8>, Option<String>)
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
                let bytes = load_binary(item_path.as_str()).unwrap();

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