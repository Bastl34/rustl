
use std::{path::Path, ffi::OsStr, sync::{Arc, RwLock}, cell::RefCell, collections::HashMap};

use gltf::{Gltf, texture, animation::util::ReadOutputs, iter::{Animations, Skins}};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::{Matrix4, Point2, Point3, Quaternion, Rotation3, UnitQuaternion, Vector2, Vector3, Vector4};
use serde_json::Value;

use crate::{component_downcast, component_downcast_mut, helper::{asset_path_descriptor::AssetPathDesciptor, change_tracker::ChangeTracker, concurrency::execution_queue::ExecutionQueueItem, file::get_stem, math::{approx_one_vec3, approx_zero_vec3}, option_or_id::OptionOrId}, resources::resources::load_binary, state::{resources::{mesh_resource::{MeshResource, MeshResourceItem}, texture::{Texture, TextureItem}, utilities::resource_utils::{insert_texture_or_reuse, load_texture_byte_or_reuse}}, scene::{camera::{Camera, CameraProjectionType}, components::{animation::{Animation, Channel, Interpolation}, component::{Component, ComponentItem}, joint::Joint, material::{BlendMode, Material, MaterialItem, TextureAddressMode, TextureFilterMode, TextureState, TextureType}, mesh::{Mesh, JOINTS_LIMIT}, morph_target::MorphTarget, transformation::Transformation}, light::Light, node::{Node, NodeItem}, scene::Scene, utilities::{extras::Extras, scene_utils::{execute_on_scene_mut_and_wait, execute_on_state_mut_and_wait}}}}};


const INTERNAL_JSON_INDEX: &str = "__internal_json_index";

pub fn load(path: &str, scene_id: u64, parent_node_id: Option<u64>, main_queue: ExecutionQueueItem, hide_root_node: bool, reuse_materials: bool, object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<Vec<u64>>
{
    println!("load gltf file {}", path);

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
    println!("loading textures...");
    let mut loaded_textures = vec![];

    for gltf_texture in gltf.textures()
    {
        let (bytes, texture_path, extension) = load_texture(path, &gltf_texture, &buffers);

        let tex = load_texture_byte_or_reuse(main_queue.clone(), max_texture_resolution, &bytes, gltf_texture.name().unwrap_or("unknown"), path, extension);
        if let Some(source) = &mut tex.write().unwrap().source
        {
            source.inner_path = texture_path.clone();
        }
        tex.write().unwrap().get_data_mut().get_mut().mipmapping = create_mipmaps;

        if tex.read().unwrap().get_data().mipmapping && tex.read().unwrap().get_data().mipmap_cache.is_none()
        {
            tex.write().unwrap().create_mipmap_cache();
        }

        // extras
        {
            let mut tex = tex.write().unwrap();
            read_extras(&mut tex.extras, gltf_texture.extras().as_ref());
        }

        loaded_textures.push((tex, gltf_texture.index()));
    }

    // because metallic and roughness are combined -> and we will use it seperatly -> the initial loaded texture should be removed again
    let mut clear_textures: Vec<TextureItem> = vec![];

    // ********** materials **********
    println!("loading materials...");
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
            let material = load_material(&gltf_material, main_queue.clone(), &loaded_textures, &mut clear_textures, create_mipmaps, max_texture_resolution, resource_name.clone().clone());
            let material_arc: MaterialItem = Arc::new(RwLock::new(Box::new(material)));

            let material_arc_clone = material_arc.clone();
            execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
            {
                scene.add_material(&material_arc_clone);
            }));

            loaded_materials.insert(gltf_material_index, material_arc);
        }
    }

    // ********** scene items **********
    println!("loading scene items...");

    // create_root_node
    let root_node = Node::new(resource_name.as_str());
    loaded_ids.push(root_node.read().unwrap().id);

    root_node.write().unwrap().root_node = true;
    root_node.write().unwrap().source = Some(AssetPathDesciptor::new_from_path(path.to_string()));

    println!("reading nodes...");
    for gltf_scene in gltf.scenes()
    {
        for node in gltf_scene.nodes()
        {
            read_node(&node, &buffers, path.to_string(), object_only, &loaded_materials, scene_id, main_queue.clone(), root_node.clone(), &Matrix4::<f32>::identity(), 1);
        }
    }

    let all_nodes = Scene::list_all_child_nodes(&root_node.read().unwrap().nodes);

    for node in all_nodes
    {
        loaded_ids.push(node.read().unwrap().id);
    }

    // ********** map skeletons **********
    println!("loading skeletons...");
    let nodes = vec![root_node.clone()];
    load_skeletons(&nodes, gltf.skins(), &buffers);

    // ********** animations **********
    println!("loading animations...");
    read_animations(root_node.clone(), gltf.animations(), &buffers);

    // ********** map animatables **********
    println!("mapping animatables...");
    map_animatables(&nodes);

    // ********** calculate skin bounding boxes **********
    println!("calc bbox skin...");
    calc_bbox_skin(&nodes);

    // ********** mark components **********
    {
        let all_nodes = Scene::list_all_child_nodes(&root_node.read().unwrap().nodes);

        for node in all_nodes
        {
            for component in &node.read().unwrap().components
            {
                let mut component = component.write().unwrap();
                component.get_base_mut().from_file = true;
            }
        }
    }

    // ********** add to scene **********
    println!("adding nodes to scene...");
    if hide_root_node
    {
        root_node.write().unwrap().visible = false;
    }

    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        if let Some(parent_node_id) = parent_node_id
        {
            let parent_node = scene.find_node_by_id(parent_node_id);
            if let Some(parent_node) = parent_node
            {
                Node::add_node(parent_node.clone(), root_node.clone());
            }
            else
            {
                dbg!("can not find parent node by id");
            }
        }
        else
        {
            scene.add_node(root_node.clone());
        }
    }));

    // ********** cleanup **********
    let mut cleanup_map = HashMap::new();

    // add cleanup textures to map
    for texture in &clear_textures
    {
        cleanup_map.insert(texture.read().unwrap().id, texture.clone());
    }

    // check if textures where loaded which are not used by any material
    for texture in loaded_textures
    {
        let mut used = false;
        for material in loaded_materials.values()
        {
            component_downcast!(material, Material);
            if material.has_texture_id(texture.0.read().unwrap().id)
            {
                used = true;
                break;
            }
        }

        if !used
        {
            cleanup_map.insert(texture.0.read().unwrap().id, texture.0.clone());
        }
    }

    println!("cleanup unused textures: {}", clear_textures.len());
    execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
    {
        for (_, clear_texture) in &cleanup_map
        {
            println!(" - texture: {} ({})", clear_texture.read().unwrap().name, clear_texture.read().unwrap().id);
            state.delete_texture_by_id(clear_texture.read().unwrap().id);
        }
    }));

    Ok(loaded_ids)
}


fn read_node(node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>, file_path: String, object_only: bool, loaded_materials: &HashMap<usize, MaterialItem>, scene_id: u64, main_queue: ExecutionQueueItem, parent: NodeItem, parent_transform: &Matrix4<f32>, level: usize)
{
    //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/mod.rs#L69

    let local_transform = transform_to_matrix(node.transform());
    //let world_transform = parent_transform * local_transform;
    let world_transform = local_transform * parent_transform;
    let (translate, rotation, scale) = transform_decompose(node.transform());

    let mut parent_node = parent;

    let node_index = node.index();

    //println!("{} - {}", " ".repeat(level * 2), node.name().unwrap_or("unknown"));

    // ********** lights **********
    if !object_only
    {
        if let Some(light) = node.light()
        {
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
                        let light = Light::new_directional((*name).clone(), pos, dir, color, intensity);
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
                        let light = Light::new_point((*name).clone(), pos, color, intensity);
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
                        let light = Light::new_spot((*name).clone(), pos, dir, color, outer_cone_angle, intensity);
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
                    let znear = ortho.znear();
                    let zfar = ortho.zfar();

                    let width = ortho.xmag();
                    let height = ortho.ymag();

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let mut cam = Camera::new((*name).clone());
                        let cam_data = cam.get_data_mut().get_mut();

                        cam_data.left = -width;
                        cam_data.right = width;
                        cam_data.top = height;
                        cam_data.bottom = -height;

                        cam_data.eye_pos = Point3::<f32>::new(pos.x, pos.y, pos.z);
                        cam_data.dir = Vector3::<f32>::new(-forward.x, -forward.y, -forward.z).normalize();
                        cam_data.up = Vector3::<f32>::new(up.x, up.y, up.z).normalize();

                        cam_data.clipping_near = znear;
                        cam_data.clipping_far = zfar;

                        cam_data.projection_type = CameraProjectionType::Orthogonal;

                        cam.init_matrices();

                        scene.cameras.push(Box::new(cam));
                    }));
                },
                gltf::camera::Projection::Perspective(pers) =>
                {
                    let yfov = pers.yfov();
                    let znear = pers.znear();
                    let zfar = pers.zfar();

                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        let mut cam = Camera::new((*name).clone());
                        let cam_data = cam.get_data_mut().get_mut();

                        cam_data.fovy = yfov;

                        cam_data.eye_pos = Point3::<f32>::new(pos.x, pos.y, pos.z);
                        cam_data.dir = Vector3::<f32>::new(-forward.x, -forward.y, -forward.z).normalize();
                        cam_data.up = Vector3::<f32>::new(up.x, up.y, up.z).normalize();

                        cam_data.clipping_near = znear;
                        cam_data.clipping_far = zfar.unwrap_or(1000.0);

                        cam_data.projection_type = CameraProjectionType::Perspective;

                        cam.init_matrices();

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

        let node_name = node.name().unwrap_or("mesh node");

        for (primitive_id, primitive) in mesh.primitives().enumerate()
        {
            let mut mesh_name = mesh.name().unwrap_or("unknown mesh").to_string();

            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let material_index = primitive.material().index();

            let mut verts: Vec<Point3::<f32>> = vec![];
            let mut uvs1: Vec<Point2<f32>> = vec![];
            let mut uvs2: Vec<Point2<f32>> = vec![];
            let mut uvs3: Vec<Point2<f32>> = vec![];
            let mut uvs4: Vec<Point2<f32>> = vec![];
            let mut normals: Vec<Vector3<f32>> = vec![];

            let mut joints: Vec<[u32; JOINTS_LIMIT]> = vec![];
            let mut weights: Vec<[f32; JOINTS_LIMIT]> = vec![];

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

            // uvs (4)
            let gltf_uvs4 = reader.read_tex_coords(3);
            if let Some(gltf_uvs4) = gltf_uvs4
            {
                for uv in gltf_uvs4.into_f32()
                {
                    // flip y coordinate
                    uvs4.push(Point2::<f32>::new(uv[0], 1.0 - uv[1]));
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

            // joints
            let joints_gltf = reader.read_joints(0); // JOINTS_0
            if let Some(joints_gltf) = joints_gltf
            {
                joints = joints_gltf.into_u16().map(|j|
                {
                    [j[0] as u32, j[1] as u32, j[2] as u32, j[3] as u32]
                }).collect();
            }

            // weights
            let weights_gltf = reader.read_weights(0);
            if let Some(weights_gltf) = weights_gltf
            {
                weights = weights_gltf.into_f32().collect();

                //normalize
                weights = weights.iter().map(|w|
                {
                    let weight = Vector4::<f32>::new(w[0], w[1], w[2], w[3]);
                    let weight = weight / weight.norm();
                    [weight.x, weight.y, weight.z, weight.w]
                }).collect::<Vec<[f32; 4]>>();
            }

            // mopth_target names
            let mut target_names: Vec<String> = vec![];
            let extras: Option<&Box<serde_json::value::RawValue>> = mesh.extras().as_ref();

            if let Some(extras) = extras
            {
                if let Ok(json) = serde_json::from_str::<Value>(extras.get())
                {
                    if let Some(names) = json["targetNames"].as_array()
                    {
                        let names = names.iter().map(|n| n.as_str().unwrap().to_string()).collect::<Vec<String>>();
                        target_names = names;
                    }
                }
            }

            let mut components: Vec<ComponentItem> = vec![];

            // mesh component
            let mut mesh_resource: MeshResource = MeshResource::new_with_data("Mesh", verts, indices, uvs1, uv_indices, normals, normals_indices);

            mesh_resource.source = Some(AssetPathDesciptor::new_from_path(file_path.clone()));
            mesh_resource.source.as_mut().unwrap().inner_path = format!("#Primitive{}", primitive_id);

            mesh_resource.get_data_mut().get_mut().uvs_1 = uvs2;
            mesh_resource.get_data_mut().get_mut().uvs_2 = uvs3;
            mesh_resource.get_data_mut().get_mut().uvs_3 = uvs4;

            if joints.len() == weights.len()
            {
                mesh_resource.get_data_mut().get_mut().joints = joints;
                mesh_resource.get_data_mut().get_mut().weights = weights;
            }
            else
            {
                println!("can not load joints and weights, because length does not match");
            }

            // morph targets
            let morpth_targets = reader.read_morph_targets();
            if morpth_targets.len() > 0
            {
                let morph_targets: Vec<_> = morpth_targets.map(|(positions, normals, tangents)|
                {
                    // positions
                    let mut res_positions: Vec<Point3<f32>> = vec![];

                    if let Some(positions) = positions
                    {
                        for position in positions
                        {
                            res_positions.push(Point3::<f32>::new(position[0], position[1], position[2]));
                        }
                    }

                    // normals
                    let mut res_normals: Vec<Vector3<f32>> = vec![];
                    if let Some(normals) = normals
                    {
                        for normal in normals
                        {
                            res_normals.push(Vector3::<f32>::new(normal[0], normal[1], normal[2]));
                        }
                    }

                    //tangents
                    let mut res_tangents: Vec<Vector3<f32>> = vec![];
                    if let Some(tangents) = tangents
                    {
                        for tangent in tangents
                        {
                            res_tangents.push(Vector3::<f32>::new(tangent[0], tangent[1], tangent[2]));
                        }
                    }

                    (res_positions, res_normals, res_tangents)
                })
                .collect();

                for (i, target) in morph_targets.iter().enumerate()
                {
                    let name = format!("Morph Target {}", i);
                    let name = target_names.get(i).unwrap_or(&name);

                    //let morph_target = MorphTarget::new(component_id, name, target.0.clone(), target.1.clone(), target.2.clone());
                    let morph_target = MorphTarget::new(name, i as u32);

                    let mesh_resource_data = mesh_resource.get_data_mut().get_mut();
                    mesh_resource_data.morph_target_positions.push(target.0.clone());
                    mesh_resource_data.morph_target_normals.push(target.1.clone());
                    mesh_resource_data.morph_target_tangents.push(target.2.clone());

                    components.push(Arc::new(RwLock::new(Box::new(morph_target))));
                }
            }

            let mesh_resource_result: Arc<RwLock<Option<MeshResourceItem>>> = Arc::new(RwLock::new(None));
            let mesh_resource_result_clone = mesh_resource_result.clone();
            let node_name_clone = node_name.to_string();

            execute_on_state_mut_and_wait(main_queue.clone(), Box::new(move |state|
            {
                let mut res = mesh_resource_result_clone.write().unwrap();
                *res = Some(state.insert_mesh_resource_or_reuse(mesh_resource, node_name_clone.as_str()));
            }));

            let mesh_resource = mesh_resource_result.read().unwrap();
            let mesh_resource_cloned = mesh_resource.as_ref().unwrap().clone();

            let mut mesh_component: Mesh = Mesh::new("Mesh");
            mesh_component.mesh_resource = OptionOrId::Some(mesh_resource_cloned);

            components.push(Arc::new(RwLock::new(Box::new(mesh_component))));

            // node
            if primitives_amount > 1
            {
                mesh_name = format!("{} {} primitive_{}", node_name, mesh_name, primitive_id);
            }

            let node_arc = Node::new(mesh_name.as_str());
            {
                let mut scene_node = node_arc.write().unwrap();

                for component in &components
                {
                    scene_node.add_component(component.clone());
                }

                scene_node.extras.insert(INTERNAL_JSON_INDEX, node_index);

                // add material
                if let Some(material_index) = material_index
                {
                    let material_arc = loaded_materials.get(&material_index).unwrap().clone();
                    scene_node.add_component(material_arc);
                }

                // transformation
                if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
                {
                    scene_node.add_component(Arc::new(RwLock::new(Box::new(Transformation::new("Transform", translate, rotation, scale)))));
                }

                // add skeleton/skin if needed
                if let Some(skin) = node.skin()
                {
                    scene_node.extras.insert("_skeleton_index", skin.index());
                }

                // add default instance
                scene_node.create_default_instance(node_arc.clone());

                // parent
                scene_node.parent = OptionOrId::Some(parent_node.clone());
            }

            // extras
            {
                let mut scene_node = node_arc.write().unwrap();
                read_extras(&mut scene_node.extras, node.extras().as_ref());
            }

            println!("{} - {} ({}) (mesh)", " ".repeat(level * 2), mesh_name.as_str(), node_index);
            Node::add_node(parent_node.clone(), node_arc.clone());

            // only if there is one primitive -> use it as parent for next childs
            if primitives_amount == 1
            {
                parent_node = node_arc.clone();
            }
        }
    }

    // ********** empty transform node **********
    // if there is nothing set -> its just a transform node
    if node.camera().is_none() && node.mesh().is_none() && node.light().is_none()
    {
        // only if the node has children -> otherwise ignore it
        //if node.children().len() > 0
        {
            let name = node.name().unwrap_or("transform node");
            println!("{} - {} ({}) (no mesh)", " ".repeat(level * 2), name, node_index);

            let scene_node = Node::new(name);
            //scene_node.write().unwrap().joint_id = Some(node.index() as u32);
            scene_node.write().unwrap().extras.insert(INTERNAL_JSON_INDEX, node_index);

            // add transformation
            if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
            {
                scene_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new("Transform", translate, rotation, scale)))));
            }

            // extras
            {
                let mut scene_node = scene_node.write().unwrap();
                read_extras(&mut scene_node.extras, node.extras().as_ref());
            }

            Node::add_node(parent_node.clone(), scene_node.clone());

            parent_node = scene_node.clone();
        }
    }

    // ********** children **********
    for child in node.children()
    {
        read_node(&child, &buffers, file_path.clone(), object_only, loaded_materials, scene_id, main_queue.clone(), parent_node.clone(), &world_transform, level + 1);
    }
}

pub fn read_extras(obj_extras: &mut Extras, gltf_extras: Option<&Box<serde_json::value::RawValue>>)
{
    if let Some(gltf_extras) = gltf_extras
    {
        if let Ok(json) = serde_json::from_str::<Value>(gltf_extras.get())
        {
            let json_content = json.as_object();

            if let Some(json_content) = json_content
            {
                for (key, value) in json_content
                {
                    if value.is_boolean()
                    {
                        obj_extras.insert::<bool>(key.as_str(), value.as_bool().unwrap());
                    }
                    else if value.is_f64()
                    {
                        obj_extras.insert::<f64>(key.as_str(), value.as_f64().unwrap());
                    }
                    else if value.is_i64()
                    {
                        obj_extras.insert::<i64>(key.as_str(), value.as_i64().unwrap());
                    }
                    else if value.is_string()
                    {
                        obj_extras.insert::<String>(key.as_str(), value.as_str().unwrap().to_string());
                    }
                    else if value.is_u64()
                    {
                        obj_extras.insert::<u64>(key.as_str(), value.as_u64().unwrap());
                    }
                    else
                    {
                        println!("extras/JSON type not supported {} {:?}", key, value);
                    }
                }
            }
        }
    }
}

pub fn read_animations(root_node: Arc<RwLock<Box<Node>>>, animations: Animations<'_>, buffers: &Vec<gltf::buffer::Data>)
{
    let all_nodes = Scene::list_all_child_nodes(&root_node.read().unwrap().nodes);

    for animation in animations
    {
        // create animation component
        let mut animation_component: Animation = Animation::new(animation.name().unwrap_or("Animation"));

        let mut duration: f32 = 0.0;

        let mut target_map: HashMap<u64, Arc<RwLock<Box<Node>>>> = HashMap::new();

        // create channels
        for channel in animation.channels()
        {
            let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
            let target = channel.target();
            let target_node_index = target.node().index();

            let mut target_node = None;

            // find target node
            for node in &all_nodes
            {
                if let Some(json_index) = node.read().unwrap().extras.get::<usize>(INTERNAL_JSON_INDEX)
                {
                    if *json_index == target_node_index
                    {
                        target_node = Some(node.clone());
                        break;
                    }
                }
            }

            if target_node.is_none()
            {
                dbg!("can not find target node");
                continue;
            }

            let target_node = target_node.unwrap();
            target_map.insert(target_node.read().unwrap().id, target_node.clone());

            let mut animation_channel = Channel::new(target_node);

            let sampler = channel.sampler();
            match sampler.interpolation()
            {
                gltf::animation::Interpolation::Linear => animation_channel.interpolation = Interpolation::Linear,
                gltf::animation::Interpolation::Step => animation_channel.interpolation = Interpolation::Step,
                gltf::animation::Interpolation::CubicSpline => animation_channel.interpolation = Interpolation::CubicSpline,
            }

            let input: Vec<_> = reader.read_inputs().unwrap().collect();
            let input_len = input.len();

            duration = duration.max(input[input_len - 1]);
            animation_channel.timestamps = input.clone();

            let output = reader.read_outputs().unwrap();

            match output
            {
                ReadOutputs::Translations(t) =>
                {
                    let trans: Vec<[f32; 3]> = t.collect();

                    animation_channel.transform_translation = trans.iter().map(|trans|
                    {
                        Vector3::<f32>::new(trans[0], trans[1], trans[2])
                    }).collect::<Vec<Vector3<f32>>>();
                },
                ReadOutputs::Rotations(r) =>
                {
                    let rot_quat: Vec<[f32; 4]> = r.into_f32().collect();

                    animation_channel.transform_rotation = rot_quat.iter().map(|rot_quat|
                    {
                        Vector4::<f32>::new(rot_quat[0], rot_quat[1], rot_quat[2], rot_quat[3])
                    }).collect::<Vec<Vector4<f32>>>();
                },
                ReadOutputs::Scales(s) =>
                {
                    let scale: Vec<[f32; 3]> = s.collect();

                    animation_channel.transform_scale = scale.iter().map(|scale|
                    {
                        Vector3::<f32>::new(scale[0], scale[1], scale[2])

                    }).collect::<Vec<Vector3<f32>>>();
                },
                ReadOutputs::MorphTargetWeights(m) =>
                {
                    let weights: Vec<_> = m.into_f32().collect();
                    let chuck_size = weights.len() / input_len;

                    let morpth_targets: Vec<Vec<f32>> = weights.chunks(chuck_size).map(|x| x.to_vec()).collect();

                    animation_channel.transform_morph = morpth_targets;
                }
            };

            animation_component.channels.push(animation_channel);
        }

        animation_component.to = duration;
        animation_component.duration = duration;


        // find best node for animation
        let mut target_nodes_vec: Vec<(u32, Arc<RwLock<Box<Node>>>)> = vec![];
        for (_, target_node) in target_map
        {
            let parent_amount = target_node.read().unwrap().parent_amount();
            target_nodes_vec.push((parent_amount, target_node.clone()));
        }

        // sort by parent amount (to find parent with the fewest parent items)
        target_nodes_vec.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // use the item with the fewest parent item as the animation node
        if let Some((parent_nodes, first)) = target_nodes_vec.first()
        {
            let parent_of_first = &first.read().unwrap().parent;
            let parent_of_first = parent_of_first.clone().unwrap();

            // root node or the first node after the root node
            if *parent_nodes <= 2
            {
                parent_of_first.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation_component))));
            }
            // otherwise use the parent of the found on in the hierarchy
            else
            {
                let parent_of_parent_first = &parent_of_first.read().unwrap().parent;
                let parent_of_parent_first = parent_of_parent_first.clone().unwrap();

                parent_of_parent_first.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation_component))));
            }
        }
        else
        {
            root_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation_component))));
        }
    }
}


fn load_skeletons(scene_nodes: &Vec<Arc<RwLock<Box<Node>>>>, skins: Skins<'_>, buffers: &Vec<gltf::buffer::Data>)
{
    let all_nodes = Scene::list_all_child_nodes(scene_nodes);
    let all_nodes_with_mesh = Scene::list_all_child_nodes_with_mesh(scene_nodes);

    let mut skin_nodes = vec![];

    for skin in skins.clone()
    {
        let skin_index = skin.index();
        println!("loading skin: {} ({})", skin.name().unwrap_or("unknown skin"), skin_index);

        // ********** load skeleton **********
        let joints = skin.joints();
        let joint_indices = joints.map(|j| j.index()).collect::<Vec<usize>>();

        let inverse_bind_matrices: Vec<_> = skin
            .reader(|b| Some(&buffers[b.index()]))
            .read_inverse_bind_matrices()
            .unwrap()
            .collect();

        let inverse_bind_matrices = inverse_bind_matrices.iter().map(|mat|
        {
            Matrix4::from_fn(|i, j| mat[j][i])
        }).collect::<Vec<Matrix4<f32>>>();

        if joint_indices.len() != inverse_bind_matrices.len()
        {
            dbg!("its not supported that joint_indices.len() != inverse_bind_matrices.len()");
            continue;
        }

        let mut joint_nodes = vec![];

        // ********** map joints **********
        for i in 0..joint_indices.len()
        {
            let joint_index = joint_indices[i];
            let inverse_bind_matrix = inverse_bind_matrices[i];

            for node_arc in &all_nodes
            {
                let mut node = node_arc.write().unwrap();

                let json_index = node.extras.get::<usize>(INTERNAL_JSON_INDEX);

                if let Some(json_index) = json_index
                {
                    if *json_index == joint_index
                    {
                        if node.find_component::<Joint>().is_none()
                        {
                            let mut joint = Joint::new("Joint");
                            joint.get_data_mut().get_mut().inverse_bind_trans = inverse_bind_matrix.clone();

                            node.add_component(Arc::new(RwLock::new(Box::new(joint))));
                        }

                        joint_nodes.push(node_arc.clone());

                        break;
                    }
                }
            }
        }

        if joint_nodes.len() != joint_indices.len()
        {
            dbg!("ERROR - something is wrong -> joint_nodes should have the same length as joint_indices");
        }

        skin_nodes.push(joint_nodes);
    }

    for (i, skin) in skins.into_iter().enumerate()
    {
        let skin_index = skin.index();

        for mesh_node in &all_nodes_with_mesh
        {
            let mut skeleton_index = None;
            {
                let mesh_node = mesh_node.read().unwrap();
                if let Some(_skeleton_index) = mesh_node.extras.get::<usize>("_skeleton_index")
                {
                    skeleton_index = Some(_skeleton_index.clone());
                }
            }

            if let Some(skeleton_index) = skeleton_index
            {
                if skeleton_index == skin_index
                {
                    let mut mesh_node = mesh_node.write().unwrap();
                    mesh_node.skin = skin_nodes[i].clone().into_iter().map(OptionOrId::Some).collect();
                }
            }
        }
    }
}

fn calc_bbox_skin(scene_nodes: &Vec<Arc<RwLock<Box<Node>>>>)
{
    let all_nodes = Scene::list_all_child_nodes(scene_nodes);
    let all_nodes_with_mesh = Scene::list_all_child_nodes_with_mesh(scene_nodes);

    // ********** update local transform for joint nodes **********
    for node in &all_nodes
    {
        let node_read = node.read().unwrap();
        if let Some(joint) = node_read.find_component::<Joint>()
        {
            let transform;
            {
                component_downcast!(joint, Joint);
                transform = joint.get_changed_local_transform(node.clone());
            }

            if let Some(transform) = transform
            {
                component_downcast_mut!(joint, Joint);
                joint.update_local_transform(transform);
            }
        }
    }

    // ********** calculate skin bounding boxes **********
    for mesh_node in &all_nodes_with_mesh
    {
        let node = mesh_node.read().unwrap();
        if node.skin.len() > 0
        {
            let joint_transform_vec = node.get_joint_transform_vec(false);

            if let Some(joint_transform_vec) = joint_transform_vec
            {
                let mesh = node.find_component::<Mesh>().unwrap();
                component_downcast_mut!(mesh, Mesh);

                mesh.calc_bbox_skin(&joint_transform_vec);
            }
        }
    }
}


fn map_animatables(scene_nodes: &Vec<Arc<RwLock<Box<Node>>>>)
{
    let all_nodes = Scene::list_all_child_nodes(scene_nodes);

    for node in &all_nodes
    {
        if let Some(animation) = node.read().unwrap().find_component::<Animation>()
        {
            component_downcast!(animation, Animation);

            for channel in &animation.channels
            {
                let target = channel.target.as_ref();
                if target.is_none() { continue; }
                let target = target.unwrap();

                // check if transformation node is existing -> if not create one
                if target.read().unwrap().find_component::<Joint>().is_none() && target.read().unwrap().find_component::<Transformation>().is_none()
                //if target.read().unwrap().find_component::<Transformation>().is_none()
                {
                    let transformation: Transformation = Transformation::identity("Animation Transformation");

                    target.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(transformation))));
                }
            }
        }
    }
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
    let quaternion = &decomposed.1;

    let quaternion = UnitQuaternion::new_normalize
    (
        Quaternion::new
        (
            quaternion[3], // W
            quaternion[0], // X
            quaternion[1], // Y
            quaternion[2], // Z
        )
    );

    let rotation: Rotation3<f32> = quaternion.into();
    let euler_angles = rotation.euler_angles();

    let rotation = Vector3::<f32>::new(euler_angles.0, euler_angles.1, euler_angles.2);

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


fn apply_texture_transform(transform: &gltf::texture::TextureTransform, tex_state: &mut TextureState)
{
    tex_state.transform.offset = Vector2::<f32>::new(transform.offset()[0], transform.offset()[1]);
    tex_state.transform.scale = Vector2::<f32>::new(transform.scale()[0], transform.scale()[1]);
    tex_state.transform.rotation = transform.rotation();

    if let Some(uv_index) = transform.tex_coord()
    {
        tex_state.transform.uv_index = uv_index;
    }
}

fn apply_texture_filtering_settings<'a>(tex_state: &mut TextureState, gltf_texture: &gltf::Texture<'a>)
{
    match gltf_texture.sampler().wrap_s()
    {
        texture::WrappingMode::ClampToEdge => tex_state.sampler.address_mode_u = TextureAddressMode::ClampToEdge,
        texture::WrappingMode::MirroredRepeat => tex_state.sampler.address_mode_u = TextureAddressMode::MirrorRepeat,
        texture::WrappingMode::Repeat => tex_state.sampler.address_mode_u = TextureAddressMode::Repeat,
    }

    match gltf_texture.sampler().wrap_t()
    {
        texture::WrappingMode::ClampToEdge => tex_state.sampler.address_mode_v = TextureAddressMode::ClampToEdge,
        texture::WrappingMode::MirroredRepeat => tex_state.sampler.address_mode_v = TextureAddressMode::MirrorRepeat,
        texture::WrappingMode::Repeat => tex_state.sampler.address_mode_v = TextureAddressMode::Repeat,
    }

    if let Some(mag_filter) = gltf_texture.sampler().mag_filter()
    {
        match mag_filter
        {
            texture::MagFilter::Nearest => tex_state.sampler.mag_filter = TextureFilterMode::Nearest,
            texture::MagFilter::Linear => tex_state.sampler.mag_filter = TextureFilterMode::Linear,
        }
    }

    if let Some(min_filter) = gltf_texture.sampler().min_filter()
    {
        match min_filter
        {
            texture::MinFilter::Nearest => tex_state.sampler.min_filter = TextureFilterMode::Nearest,
            texture::MinFilter::Linear => tex_state.sampler.min_filter = TextureFilterMode::Linear,
            texture::MinFilter::NearestMipmapNearest =>
            {
                tex_state.sampler.min_filter = TextureFilterMode::Nearest;
                tex_state.sampler.mipmap_filter = TextureFilterMode::Nearest;
            },
            texture::MinFilter::LinearMipmapNearest =>
            {
                tex_state.sampler.min_filter = TextureFilterMode::Linear;
                tex_state.sampler.mipmap_filter = TextureFilterMode::Nearest;
            },
            texture::MinFilter::NearestMipmapLinear =>
            {
                tex_state.sampler.min_filter = TextureFilterMode::Nearest;
                tex_state.sampler.mipmap_filter = TextureFilterMode::Linear;
            },
            texture::MinFilter::LinearMipmapLinear =>
            {
                tex_state.sampler.min_filter = TextureFilterMode::Linear;
                tex_state.sampler.mipmap_filter = TextureFilterMode::Linear;
            },
        }
    }
}

pub fn load_material(gltf_material: &gltf::Material<'_>, main_queue: ExecutionQueueItem, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>, clear_textures: &mut Vec<TextureItem>, create_mipmaps: bool, max_texture_resolution: u32, resource_name: String) -> Material
{
    let mut material = Material::new(gltf_material.name().unwrap_or("unknown"));
    let material_name = material.get_base().name.clone();
    let material_data = material.get_data_mut().get_mut();

    let base_color = gltf_material.pbr_metallic_roughness().base_color_factor();
    material_data.base_color = Vector3::<f32>::new(base_color[0], base_color[1], base_color[2]);
    material_data.alpha = base_color[3];

    material_data.blend_mode = match gltf_material.alpha_mode()
    {
        gltf::material::AlphaMode::Blend => BlendMode::Blend,
        gltf::material::AlphaMode::Mask => BlendMode::Mask,
        gltf::material::AlphaMode::Opaque => BlendMode::Opaque
    };

    material_data.alpha_cutoff = gltf_material.alpha_cutoff();

    //default alpha cutoff is 0.5 for mask blend mode
    // https://github.com/KhronosGroup/glTF-Sample-Models/blob/main/2.0/AlphaBlendModeTest/README.md#problem-no-default-cutoff
    if material_data.blend_mode == BlendMode::Mask && material_data.alpha_cutoff.is_none()
    {
        material_data.alpha_cutoff = Some(0.5);
    }

    // base/albedo texture
    if let Some(tex) = gltf_material.pbr_metallic_roughness().base_color_texture()
    {
        if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
        {
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Base);
            material_data.texture_base = Some(TextureState::new(texture));

            apply_texture_filtering_settings(material_data.texture_base.as_mut().unwrap(), &tex.texture());

            if let Some(transform) = tex.texture_transform()
            {
                let tex = material_data.texture_base.as_mut().unwrap();
                apply_texture_transform(&transform, tex);
            }
        }
    }

    // normal
    if let Some(tex) = gltf_material.normal_texture()
    {
        if let Some(texture) = get_normal_texture_by_index(&tex, &loaded_textures)
        {
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Normal);
            material_data.texture_normal = Some(TextureState::new(texture));

            apply_texture_filtering_settings(material_data.texture_normal.as_mut().unwrap(), &tex.texture());

            /*
            // uncomment when this is merged: https://github.com/gltf-rs/gltf/pull/394
            if let Some(transform) = tex.texture_transform()
            {
                let tex = data.texture_normal.as_mut().unwrap();
                apply_texture_transform(&transform, tex);
            }
            */
        }
    }

    // specular
    let specular = gltf_material.specular();
    if let Some(specular) = specular
    {
        // https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_materials_specular/README.md
        let specular_color = specular.specular_color_factor();
        let specular_color_factor = specular.specular_factor();

        material_data.specular_color = Vector3::<f32>::new(specular_color[0] * specular_color_factor, specular_color[1] * specular_color_factor, specular_color[2] * specular_color_factor);

        if let Some(specular_tex) = specular.specular_color_texture()
        {
            if let Some(texture) = get_texture_by_index(&specular_tex, &loaded_textures)
            {
                set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Specular);
                material_data.texture_specular = Some(TextureState::new(texture));

                apply_texture_filtering_settings(material_data.texture_specular.as_mut().unwrap(), &specular_tex.texture());

                if let Some(transform) = specular_tex.texture_transform()
                {
                    let tex = material_data.texture_specular.as_mut().unwrap();
                    apply_texture_transform(&transform, tex);
                }
            }
        }
    }
    else
    {
        // if there is no specular color -> use base color
        material_data.specular_color = material_data.base_color * 0.8;
    }

    // reflectivity (metallic and roughness are combined in the loaded texture)
    // do not use full metallic_factor as reflectivity --> otherwise the object will be just complete mirror if metallic is set to 1.0
    //data.reflectivity = gltf_material.pbr_metallic_roughness().metallic_factor() * 0.5; // TODO CHECK ME
    material_data.reflectivity = gltf_material.pbr_metallic_roughness().metallic_factor();

    if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
    {
        if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
        {
            let reflectivity_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                reflectivity_tex = Texture::new_from_image_channel(tex.name.as_str(), &tex, 2, max_texture_resolution);
            }
            let tex_arc: Arc<RwLock<Box<Texture>>> = insert_texture_or_reuse(main_queue.clone(), reflectivity_tex, tex_name.as_str());

            // create mipmap cache
            if create_mipmaps && !tex_arc.read().unwrap().get_data().mipmap_cache.is_none()
            {
                tex_arc.write().unwrap().create_mipmap_cache();
            }

            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            if let Some(source) = &mut tex_arc.write().unwrap().source
            {
                source.variation = "Reflectivity".to_string();
            }

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::Reflectivity);
            material_data.texture_reflectivity = Some(TextureState::new(tex_arc));

            apply_texture_filtering_settings(material_data.texture_reflectivity.as_mut().unwrap(), &metallic_roughness_tex.texture());

            if let Some(transform) = metallic_roughness_tex.texture_transform()
            {
                let tex = material_data.texture_reflectivity.as_mut().unwrap();
                apply_texture_transform(&transform, tex);
            }

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // roughness (metallic and roughness are combined in the loaded texture)
    material_data.roughness = gltf_material.pbr_metallic_roughness().roughness_factor();

    if let Some(metallic_roughness_tex) = gltf_material.pbr_metallic_roughness().metallic_roughness_texture()
    {
        if let Some(texture) = get_texture_by_index(&metallic_roughness_tex, &loaded_textures)
        {
            let roughness_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                roughness_tex = Texture::new_from_image_channel(tex.name.as_str(), &tex, 1, max_texture_resolution);
            }
            let tex_arc = insert_texture_or_reuse(main_queue.clone(), roughness_tex, tex_name.as_str());

            // create mipmap cache
            if create_mipmaps && !tex_arc.read().unwrap().get_data().mipmap_cache.is_none()
            {
                tex_arc.write().unwrap().create_mipmap_cache();
            }

            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            if let Some(source) = &mut tex_arc.write().unwrap().source
            {
                source.variation = "Roughness".to_string();
            }

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::Roughness);
            material_data.texture_roughness = Some(TextureState::new(tex_arc));

            apply_texture_filtering_settings(material_data.texture_roughness.as_mut().unwrap(), &metallic_roughness_tex.texture());

            if let Some(transform) = metallic_roughness_tex.texture_transform()
            {
                let tex = material_data.texture_roughness.as_mut().unwrap();
                apply_texture_transform(&transform, tex);
            }

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // emissive / ambient
    let emissive = gltf_material.emissive_factor();
    material_data.ambient_color = Vector3::<f32>::new(emissive[0], emissive[1], emissive[2]);

    if let Some(tex) = gltf_material.emissive_texture()
    {
        if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
        {
            set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::AmbientEmissive);
            material_data.texture_ambient = Some(TextureState::new(texture));

            apply_texture_filtering_settings(material_data.texture_ambient.as_mut().unwrap(), &tex.texture());

            if let Some(transform) = tex.texture_transform()
            {
                let tex = material_data.texture_ambient.as_mut().unwrap();
                apply_texture_transform(&transform, tex);
            }
        }
    }

    // ambient occlusion
    if let Some(ao_gltf_tex) = gltf_material.occlusion_texture()
    {
        if let Some(texture) = get_ao_texture_by_index(&ao_gltf_tex, &loaded_textures)
        {
            //data.texture_ambient_occlusion = Some(TextureState::new(texture));
            let ao_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                ao_tex = Texture::new_from_image_channel(tex.name.as_str(), &tex, 0, max_texture_resolution);
            }
            let tex_arc: Arc<RwLock<Box<Texture>>> = insert_texture_or_reuse(main_queue.clone(), ao_tex, tex_name.as_str());

            // create mipmap cache
            if create_mipmaps && !tex_arc.read().unwrap().get_data().mipmap_cache.is_none()
            {
                tex_arc.write().unwrap().create_mipmap_cache();
            }

            tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

            set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::AmbientOcclusion);
            material_data.texture_ambient_occlusion = Some(TextureState::new(tex_arc));

            apply_texture_filtering_settings(material_data.texture_ambient_occlusion.as_mut().unwrap(), &ao_gltf_tex.texture());

            /*
            // uncomment when this is merged: https://github.com/gltf-rs/gltf/pull/394
            if let Some(transform) = ao_gltf_tex.texture_transform()
            {
                let tex = &data.texture_ambient_occlusion.as_mut().unwrap().item;
                apply_texture_transform(&transform, tex.clone());
            }
            */

            // add texture to clearable textures
            clear_textures.push(texture.clone());
        }
    }

    // pbr specular glossiness
    if let Some(pbr_specular_glossiness) = gltf_material.pbr_specular_glossiness()
    {
        let base_color = pbr_specular_glossiness.diffuse_factor();
        material_data.base_color = Vector3::<f32>::new(base_color[0], base_color[1], base_color[2]);

        // diffuse to --> base/albedo texture
        if let Some(tex) = pbr_specular_glossiness.diffuse_texture()
        {
            if let Some(texture) = get_texture_by_index(&tex, &loaded_textures)
            {
                set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Base);
                material_data.texture_base = Some(TextureState::new(texture));

                apply_texture_filtering_settings(material_data.texture_base.as_mut().unwrap(), &tex.texture());

                if let Some(transform) = tex.texture_transform()
                {
                    let tex = material_data.texture_base.as_mut().unwrap();
                    apply_texture_transform(&transform, tex);
                }
            }
        }

        // specular color
        let specular_color_factor = pbr_specular_glossiness.specular_factor();
        let glossiness_color_factor = pbr_specular_glossiness.glossiness_factor();

        material_data.base_color = Vector3::<f32>::new(specular_color_factor[0], specular_color_factor[1], specular_color_factor[2]);
        material_data.specular_color = Vector3::<f32>::new(specular_color_factor[0], specular_color_factor[1], specular_color_factor[2]);

        material_data.roughness = 1.0 - glossiness_color_factor;

        // specular-glossiness texture is an RGBA texture, containing the specular color (RGB) encoded with the sRGB transfer function and the linear glossiness value (A).
        if let Some(specular_glossiness_texture) = pbr_specular_glossiness.specular_glossiness_texture()
        {
            // roughness is stored in the alpha channel (3) of the specular-glossiness texture
            if let Some(texture) = get_texture_by_index(&specular_glossiness_texture, &loaded_textures)
            {
                let new_tex;
                let tex_name;
                {
                    let tex = texture.read().unwrap();
                    tex_name = tex.name.clone();
                    new_tex = Texture::new_from_image_channel(tex.name.as_str(), &tex, 3, max_texture_resolution);
                }
                let tex_arc = insert_texture_or_reuse(main_queue.clone(), new_tex, tex_name.as_str());

                // create mipmap cache
                if create_mipmaps && !tex_arc.read().unwrap().get_data().mipmap_cache.is_none()
                {
                    tex_arc.write().unwrap().create_mipmap_cache();
                }

                tex_arc.write().unwrap().data.get_mut().mipmapping = create_mipmaps;

                if let Some(source) = &mut tex_arc.write().unwrap().source
                {
                    source.variation = "Reflecivity".to_string();
                }

                set_texture_name(tex_arc.clone(), material_name.clone(), resource_name.clone(), TextureType::Reflectivity);
                material_data.texture_reflectivity = Some(TextureState::new(tex_arc));

                apply_texture_filtering_settings(material_data.texture_reflectivity.as_mut().unwrap(), &specular_glossiness_texture.texture());

                if let Some(transform) = specular_glossiness_texture.texture_transform()
                {
                    let tex = material_data.texture_reflectivity.as_mut().unwrap();
                    apply_texture_transform(&transform, tex);
                }
            }

            // specular color is stored in the RGB channels (0, 1, 2) of the specular-glossiness texture
            // use RGBA even if A is not used
            if let Some(texture) = get_texture_by_index(&specular_glossiness_texture, &loaded_textures)
            {
                texture.write().unwrap().make_fully_opaque();

                set_texture_name(texture.clone(), material_name.clone(), resource_name.clone(), TextureType::Specular);
                material_data.texture_specular = Some(TextureState::new(texture));

                apply_texture_filtering_settings(material_data.texture_specular.as_mut().unwrap(), &specular_glossiness_texture.texture());

                if let Some(transform) = specular_glossiness_texture.texture_transform()
                {
                    let tex = material_data.texture_base.as_mut().unwrap();
                    apply_texture_transform(&transform, tex);
                }
            }
        }
    }

    // backface culling
    material_data.backface_culling = !gltf_material.double_sided();

    // index of refraction
    if let Some(ior) = gltf_material.ior()
    {
        material_data.refraction_index = ior;
    }

    // unlit
    material_data.unlit_shading = gltf_material.unlit();

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
pub fn load_texture(gltf_path: &str, texture: &gltf::Texture<'_>, buffers: &Vec<gltf::buffer::Data>) -> (Vec<u8>, String, Option<String>)
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

            (data.to_vec(), format!("#ImageView{}",texture.index()), extension.map(str::to_string))
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

                (data, format!("#ImageData_{}",texture.index()), extension.map(str::to_string))
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
                    (bytes, item_path, extension.map(str::to_string))
                }
                else
                {
                    (bytes, item_path, None)
                }
            }
        }
    }
}
