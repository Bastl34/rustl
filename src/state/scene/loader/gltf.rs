
use std::{path::Path, ffi::OsStr, sync::{Arc, RwLock}, cell::RefCell, collections::HashMap};

use gltf::{Gltf, texture, mesh::util::{weights}, animation::util::ReadOutputs, iter::{Animations, Skins}};

use base64::{engine::general_purpose::STANDARD, Engine};
use nalgebra::{Vector3, Matrix4, Point3, Point2, UnitQuaternion, Quaternion, Rotation3, Vector4};

use crate::{state::scene::{scene::Scene, components::{material::{Material, MaterialItem, TextureState, TextureType}, mesh::{Mesh, JOINTS_LIMIT}, transformation::Transformation, component::Component, joint::Joint, animation::{Animation, Channel, Interpolation}}, texture::{Texture, TextureItem, TextureAddressMode, TextureFilterMode}, light::Light, camera::Camera, node::{NodeItem, Node}, utilities::scene_utils::{load_texture_byte_or_reuse, execute_on_scene_mut_and_wait, insert_texture_or_reuse}, manager::id_manager::IdManagerItem}, resources::resources::load_binary, helper::{change_tracker::ChangeTracker, math::{approx_zero_vec3, approx_one_vec3}, file::get_stem, concurrency::execution_queue::ExecutionQueueItem}, rendering::{scene, light, skeleton}, component_downcast_mut, component_downcast};


struct JointData
{
    index: usize,
    inverse_bind_matrix: Matrix4<f32>
}

type Skeletons = HashMap<usize, Vec<JointData>>;

pub fn load(path: &str, scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, reuse_materials: bool, object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<Vec<u64>>
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

        let tex = load_texture_byte_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, &bytes, gltf_texture.name().unwrap_or("unknown"), extension);
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
            let material = load_material(&gltf_material, scene_id, main_queue.clone(), id_manager.clone(), &loaded_textures, &mut clear_textures, create_mipmaps, max_texture_resolution, resource_name.clone().clone());
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

    // create_root_node
    let node_id = id_manager.write().unwrap().get_next_node_id();
    loaded_ids.push(node_id);

    let root_node = Node::new(node_id, resource_name.as_str());
    root_node.write().unwrap().root_node = true;

    dbg!("reading nodes...");
    for gltf_scene in gltf.scenes()
    {
        for node in gltf_scene.nodes()
        {
            read_node(&node, &buffers, object_only, &loaded_materials, scene_id, main_queue.clone(), id_manager.clone(), root_node.clone(), &Matrix4::<f32>::identity(), 1);
        }
    }

    let all_nodes = Scene::list_all_child_nodes(&root_node.read().unwrap().nodes);

    for node in all_nodes
    {
        loaded_ids.push(node.read().unwrap().id);
    }

    // ********** map skeletons **********
    dbg!("loading skeletons...");
    let nodes = vec![root_node.clone()];
    load_skeletons(&nodes, gltf.skins(), &buffers, id_manager.clone());

    // ********** animations **********
    dbg!("loading animations...");
    read_animations(root_node.clone(), id_manager.clone(), gltf.animations(), &buffers);

    // ********** map animatables **********
    dbg!("mapping animatables...");
    map_animatables(&nodes, id_manager.clone());

    // ********** add to scene **********
    dbg!("adding nodes to scene...");
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        scene.add_node(root_node.clone());
    }));

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


fn read_node(node: &gltf::Node, buffers: &Vec<gltf::buffer::Data>, object_only: bool, loaded_materials: &HashMap<usize, MaterialItem>, scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, parent: NodeItem, parent_transform: &Matrix4<f32>, level: usize)
{
    //https://github.com/flomonster/easy-gltf/blob/de8654c1d3f069132dbf1bf3b50b1868f6cf1f84/src/scene/mod.rs#L69

    let local_transform = transform_to_matrix(node.transform());
    //let world_transform = parent_transform * local_transform;
    let world_transform = local_transform * parent_transform;
    let (translate, rotation, scale) = transform_decompose(node.transform());

    let mut parent_node = parent;

    let node_index = node.index();

    // ********** lights **********
    if !object_only
    {
        if let Some(light) = node.light()
        {
            let light_id = id_manager.write().unwrap().get_next_light_id();
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
            let cam_id = id_manager.write().unwrap().get_next_camera_id();
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

            if verts.len() == 0 || indices.len() == 0
            {
                continue;
            }

            let component_id = id_manager.write().unwrap().get_next_component_id();
            let mut item = Mesh::new_with_data(component_id, "Mesh", verts, indices, uvs1, uv_indices, normals, normals_indices);
            item.get_data_mut().get_mut().uvs_2 = uvs2;
            item.get_data_mut().get_mut().uvs_3 = uvs3;

            if joints.len() == weights.len()
            {
                item.get_data_mut().get_mut().joints = joints;
                item.get_data_mut().get_mut().weights = weights;
            }
            else
            {
                println!("can not load joints and weights, because length does not match");
            }

            let id = id_manager.write().unwrap().get_next_node_id();

            if primitives_amount > 1
            {
                name = format!("{} primitive_{}", name, primitive_id);
            }

            let node_arc = Node::new(id, name.as_str());
            {
                let mut scene_node = node_arc.write().unwrap();
                scene_node.add_component(Arc::new(RwLock::new(Box::new(item))));
                scene_node.extras.insert("_json_index".to_string(), node_index.to_string());

                // add material
                if let Some(material_index) = material_index
                {
                    let material_arc = loaded_materials.get(&material_index).unwrap().clone();
                    scene_node.add_component(material_arc);
                }

                // transformation
                if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
                {
                    let component_id = id_manager.write().unwrap().get_next_component_id();
                    scene_node.add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform", translate, rotation, scale)))));
                }

                // add skeleton/skin if needed
                if let Some(skin) = node.skin()
                {
                    dbg!(" --- {} {}", name, skin.index());
                    scene_node.extras.insert("_skeleton_index".to_string(), skin.index().to_string());
                }

                // add default instance
                let instance_id = id_manager.write().unwrap().get_next_instance_id();
                scene_node.create_default_instance(node_arc.clone(), instance_id);

                // parent
                scene_node.parent = Some(parent_node.clone());
            }

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
            println!("load empty {} {}", name, node.index());

            let node_id = id_manager.write().unwrap().get_next_node_id();
            let scene_node = Node::new(node_id, name);
            //scene_node.write().unwrap().joint_id = Some(node.index() as u32);
            scene_node.write().unwrap().extras.insert("_json_index".to_string(), node_index.to_string());

            // add transformation
            if !approx_zero_vec3(&translate) || !approx_zero_vec3(&rotation) || !approx_one_vec3(&scale)
            {
                let component_id = id_manager.write().unwrap().get_next_component_id();
                scene_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(Transformation::new(component_id, "Transform", translate, rotation, scale)))));
            }

            Node::add_node(parent_node.clone(), scene_node.clone());

            parent_node = scene_node.clone();
        }
    }

    // ********** children **********
    for child in node.children()
    {
        read_node(&child, &buffers, object_only, loaded_materials, scene_id, main_queue.clone(), id_manager.clone(), parent_node.clone(), &world_transform, level + 1);
    }
}

pub fn read_animations(root_node: Arc<RwLock<Box<Node>>>, id_manager: IdManagerItem, animations: Animations<'_>, buffers: &Vec<gltf::buffer::Data>)
{
    let all_nodes = Scene::list_all_child_nodes(&root_node.read().unwrap().nodes);

    for animation in animations
    {
        // create animation component
        let component_id = id_manager.write().unwrap().get_next_component_id();
        let mut animation_component: Animation = Animation::new(component_id, animation.name().unwrap_or("Animation"));

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
                if let Some(json_index) = node.read().unwrap().extras.get("_json_index")
                {
                    let json_index = json_index.parse::<usize>().unwrap();
                    if json_index == target_node_index
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
                // find best item based on parents
                /*
                let mut possible_node = parent_of_first.clone();
                let mut parent_nodes = *parent_nodes;
                while parent_nodes > 2
                {
                    if possible_node.read().unwrap().parent.is_some()
                    {
                        let parent;
                        {
                            let parent_arc = &possible_node.read().unwrap().parent;
                            parent = parent_arc.clone().unwrap();
                        }
                        possible_node = parent;
                        parent_nodes -= 1;
                    }
                    else
                    {
                        break;
                    }
                }
                possible_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation_component))));
                */
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


fn load_skeletons(scene_nodes: &Vec<Arc<RwLock<Box<Node>>>>, skins: Skins<'_>, buffers: &Vec<gltf::buffer::Data>, id_manager: IdManagerItem)
{
    let all_nodes = Scene::list_all_child_nodes(scene_nodes);
    let all_nodes_with_mesh = Scene::list_all_child_nodes_with_mesh(scene_nodes);

    for skin in skins
    {
        let skin_index = skin.index();
        dbg!("loading skin: {}", skin.name().unwrap_or("unknown skin"));

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

        // ********** map joints **********
        for i in 0..joint_indices.len()
        {
            let joint_id = i;
            let joint_index = joint_indices[i];
            let inverse_bind_matrix = inverse_bind_matrices[i];

            for node in &all_nodes
            {
                let mut node = node.write().unwrap();

                let json_index = node.extras.get("_json_index");

                if let Some(json_index) = json_index
                {
                    let json_index = json_index.parse::<usize>().unwrap();

                    if json_index == joint_index
                    {
                        if node.find_component::<Joint>().is_none()
                        {
                            let component_id = id_manager.write().unwrap().get_next_component_id();
                            let mut joint = Joint::new(component_id, "Joint", joint_id as u32);
                            joint.get_data_mut().get_mut().inverse_bind_trans = inverse_bind_matrix.clone();

                            node.add_component(Arc::new(RwLock::new(Box::new(joint))));
                        }

                        let joint = node.find_component::<Joint>().unwrap();
                        component_downcast_mut!(joint, Joint);
                        let joint_data = joint.get_data_mut().get_mut();
                        joint_data.skin_ids.insert(skin_index as u32);
                    }
                }
            }
        }

        // ********** map skeletons (root skeleton nodes) **********
        for mesh_node in &all_nodes_with_mesh
        {
            //scene_node.extras.insert("_skeleton_index".to_string(), skin.index().to_string());
            let mut skeleton_index = None;
            {
                let mesh_node = mesh_node.read().unwrap();
                if let Some(_skeleton_index) = mesh_node.extras.get("_skeleton_index")
                {
                    skeleton_index = Some(_skeleton_index.clone());
                }
            }

            if let Some(skeleton_index) = skeleton_index
            {
                let skeleton_index = skeleton_index.parse::<usize>().unwrap();

                //dbg!(skeleton_index);

                // INFO: this is only working because all_nodes adds parent first and next items are the child items of the parent
                for node_arc in &all_nodes
                {
                    let node = node_arc.read().unwrap();
                    if let Some(joint) = node.find_component::<Joint>()
                    {
                        component_downcast!(joint, Joint);
                        let joint_data = joint.get_data();

                        if joint_data.skin_ids.contains(&(skeleton_index as u32))
                        {
                            let mut mesh_node = mesh_node.write().unwrap();
                            mesh_node.skin_root_node = Some(node_arc.clone());
                            //mesh_node.skin_root_id = Some(skin_index as u32);
                            break;
                        }
                    }
                }


                /*
                if skeleton_index == skin.index()
                {
                    let mut node = node.write().unwrap();

                    let parent = node.parent.clone();

                    if parent.is_none()
                    {
                        node.skin_root_node = Some(node.clone());
                    }
                    else
                    {
                        let parent = parent.unwrap();

                        if parent.read().unwrap().find_component::<Joint>().is_none()
                        {
                            node.skin_root_node = Some(node.clone());
                        }
                    }
                }
                 */
            }
        }
    }


    /*

        // ********** skin/skeleton **********
    /*
    if let Some(skin) = node.skin()
    {
        let joints = skin.joints();
        let joint_indices = joints.map(|j| j.index()).collect::<Vec<usize>>();

        dbg!("skin: {} joints {}", skin.index(), joint_indices.len());
        dbg!(&joint_indices);

        let inverse_bind_matrices: Vec<_> = skin
            .reader(|b| Some(&buffers[b.index()]))
            .read_inverse_bind_matrices()
            .unwrap()
            .collect();

        let inverse_bind_matrices = inverse_bind_matrices.iter().map(|mat|
        {
            Matrix4::from_fn(|i, j| mat[j][i])
        }).collect::<Vec<Matrix4<f32>>>();

        if inverse_bind_matrices.len() == joint_indices.len()
        {
            let inverse_bind_matrices = inverse_bind_matrices.iter().enumerate().map(|(i, mat)|
            {
                JointData
                {
                    index: joint_indices[i],
                    inverse_bind_matrix: *mat
                }
            }).collect::<Vec<JointData>>();

            skeletons.insert(node_index, inverse_bind_matrices);
        }
        else
        {
            dbg!("joints len does not match inverse_bind_matrices len");
        }
    }
     */
     */

    /*
    dbg!(skeletons.len());

    for mesh_node in &all_nodes_with_mesh
    {
        let mesh_node_json_index = mesh_node.read().unwrap().extras.get("_json_index").unwrap().parse::<u32>().unwrap();

        // ******************** map joint_ids ********************
        if let Some(mesh_joint_data) = skeletons.get(&(mesh_node_json_index as usize))
        {
            for (joint_id, mesh_joint_data) in mesh_joint_data.iter().enumerate()
            {
                for node_arc in &all_nodes
                {
                    let mut json_index = "".to_string();
                    let has_joint;
                    {
                        let node = node_arc.read().unwrap();
                        let _json_index = node.extras.get("_json_index");
                        if let Some(_json_index) = _json_index
                        {
                            json_index = _json_index.clone();
                        }

                        has_joint = node_arc.read().unwrap().find_component::<Joint>().is_some();
                    }

                    if json_index.len() > 0 && !has_joint
                    {
                        let json_index = json_index.parse::<u32>().unwrap();

                        if json_index == mesh_joint_data.index as u32
                        {
                            let component_id = id_manager.write().unwrap().get_next_component_id();
                            let mut joint = Joint::new(component_id, "Joint", joint_id as u32);
                            joint.get_data_mut().get_mut().inverse_bind_trans = mesh_joint_data.inverse_bind_matrix.clone();

                            node_arc.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(joint))));
                        }
                    }
                }
            }
        }

        // ******************** map skin_root_node and find root joint ********************
        if let Some(mesh_joint_data) = skeletons.get(&(mesh_node_json_index as usize))
        {
            for (joint_id, _mesh_joint_data) in mesh_joint_data.iter().enumerate()
            {
                for node_arc in &all_nodes
                {
                    let joint_component = node_arc.read().unwrap().find_component::<Joint>();

                    if joint_component.is_none()
                    {
                        continue;
                    }

                    let joint_component = joint_component.unwrap();
                    component_downcast_mut!(joint_component, Joint);

                    let parent;
                    {
                        let node = node_arc.read().unwrap();
                        parent = node.parent.clone();
                    }

                    if joint_id as u32 == joint_component.get_data().joint_id
                    {
                        // the node has no parent and joint id is matching
                        if parent.is_none()
                        {
                            //joint_component.get_data_mut().get_mut().skin_root_node = Some(node_arc.clone());
                            mesh_node.write().unwrap().skin_root_node = Some(node_arc.clone());
                        }

                        // node has parent but parent is no joint
                        if let Some(parent) = &parent
                        {
                            if parent.read().unwrap().find_component::<Joint>().is_none()
                            {
                                mesh_node.write().unwrap().skin_root_node = Some(node_arc.clone());
                            }
                        }
                    }
                }
            }
        }
    }
     */
}


fn map_animatables(scene_nodes: &Vec<Arc<RwLock<Box<Node>>>>, id_manager: IdManagerItem)
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

                // check if transformation node is existing -> if not create one
                if target.read().unwrap().find_component::<Joint>().is_none() && target.read().unwrap().find_component::<Transformation>().is_none()
                //if target.read().unwrap().find_component::<Transformation>().is_none()
                {
                    let component_id = id_manager.write().unwrap().get_next_component_id();
                    let transformation: Transformation = Transformation::identity(component_id, "Animation Transformation");

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

pub fn load_material(gltf_material: &gltf::Material<'_>, scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, loaded_textures: &Vec<(Arc<RwLock<Box<Texture>>>, usize)>, clear_textures: &mut Vec<TextureItem>, create_mipmaps: bool, max_texture_resolution: u32, resource_name: String) -> Material
{
    //let component_id = scene.id_manager.get_next_component_id();
    let component_id: u64 = id_manager.write().unwrap().get_next_component_id();

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
            let tex_id: u64 = id_manager.write().unwrap().get_next_texture_id();

            let reflectivity_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                reflectivity_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 2, max_texture_resolution);
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
            let tex_id: u64 = id_manager.write().unwrap().get_next_texture_id();

            let roughness_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                roughness_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 1, max_texture_resolution);
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
            let tex_id: u64 = id_manager.write().unwrap().get_next_texture_id();

            //data.texture_ambient_occlusion = Some(TextureState::new(texture));
            let ao_tex;
            let tex_name;
            {
                let tex = texture.read().unwrap();
                tex_name = tex.name.clone();
                ao_tex = Texture::new_from_image_channel(tex_id, tex.name.as_str(), &tex, 0, max_texture_resolution);
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
