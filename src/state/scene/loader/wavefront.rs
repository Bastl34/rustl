use std::{io::{Cursor, BufReader}, sync::{RwLock, Arc}, path::Path};

use nalgebra::{Point3, Point2, Vector3};

use crate::{resources::resources::load_string, state::scene::{components::{mesh::Mesh, material::{Material, TextureType, MaterialItem}, component::Component}, scene::Scene, node::Node, utilities::scene_utils::{load_texture_or_reuse, execute_on_scene_mut_and_wait}, manager::id_manager::IdManagerItem}, helper::{self, concurrency::execution_queue::ExecutionQueueItem, file::get_stem}, new_component};

pub fn get_texture_path(tex_path: &String, mtl_path: &str) -> String
{
    let mut tex_path = tex_path.clone();

    if Path::new(&tex_path).is_relative()
    {
        let parent = Path::new(mtl_path).parent();
        if let Some(parent) = parent
        {
            tex_path = parent.join(tex_path).to_str().unwrap().to_string();
        }
    }

    tex_path
}

pub fn load(path: &str, scene_id: u64, main_queue: ExecutionQueueItem, id_manager: IdManagerItem, reuse_materials: bool, _object_only: bool, create_mipmaps: bool, max_texture_resolution: u32) -> anyhow::Result<Vec<u64>>
{
    let mut loaded_ids: Vec<u64> = vec![];

    let resource_name = get_stem(path);

    let obj_text = load_string(path)?;
    let obj_cursor = Cursor::new(obj_text);
    let mut obj_reader = BufReader::new(obj_cursor);

    let (models, materials) = tobj::load_obj_buf
    (
        &mut obj_reader,
        &tobj::LoadOptions
        {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        move |p|
        {
            let mut file_path = p.to_str().unwrap().to_string();
            if !helper::file::is_absolute(file_path.as_str())
            {
                file_path = helper::file::get_dirname(path) + "/" + &file_path;
            }

            let mat_text = load_string(&file_path).unwrap();
            tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(mat_text)))
        },
    )?;

    let wavefront_materials = materials.unwrap();
    let mut scene_nodes = vec![];

    let mut double_check_materials: Vec<(usize, MaterialItem)> = vec![];

    for (_i, m) in models.iter().enumerate()
    {
        let mesh = &m.mesh;

        if mesh.texcoord_indices.len() > 0 && mesh.indices.len() != mesh.texcoord_indices.len()
        {
            println!("Error can not load {}, because of indices mismatch", m.name.as_str());
            continue;
        }

        let mut verts: Vec<Point3::<f32>> = vec![];
        let mut uvs: Vec<Point2<f32>> = vec![];
        let mut normals: Vec<Vector3<f32>> = vec![];

        let mut indices:Vec<[u32; 3]> = vec![];
        let mut uv_indices: Vec<[u32; 3]> = vec![];
        let mut normals_indices: Vec<[u32; 3]> = vec![];


        //vertices
        for vtx in 0..mesh.positions.len() / 3
        {
            let x = mesh.positions[3 * vtx];
            let y = mesh.positions[3 * vtx + 1];
            let z = mesh.positions[3 * vtx + 2];

            verts.push(Point3::<f32>::new(x, y, z));
        }

        //normals
        for vtx in 0..mesh.normals.len() / 3
        {
            let x = mesh.normals[3 * vtx];
            let y = mesh.normals[3 * vtx + 1];
            let z = mesh.normals[3 * vtx + 2];

            normals.push(Vector3::<f32>::new(x, y, z));
        }

        //tex coords
        for vtx in 0..mesh.texcoords.len() / 2
        {
            let x = mesh.texcoords[2 * vtx];
            let y = mesh.texcoords[2 * vtx + 1];

            uvs.push(Point2::<f32>::new(x, y));
        }

        //indices
        for vtx in 0..mesh.indices.len() / 3
        {
            let i0 = mesh.indices[3 * vtx];
            let i1 = mesh.indices[3 * vtx + 1];
            let i2 = mesh.indices[3 * vtx + 2];

            indices.push([i0, i1, i2]);
        }

        //tex coords indices
        for vtx in 0..mesh.texcoord_indices.len() / 3
        {
            let i0 = mesh.texcoord_indices[3 * vtx];
            let i1 = mesh.texcoord_indices[3 * vtx + 1];
            let i2 = mesh.texcoord_indices[3 * vtx + 2];

            uv_indices.push([i0, i1, i2]);
        }

        //normals coords indices
        for vtx in 0..mesh.normal_indices.len() / 3
        {
            let i0 = mesh.normal_indices[3 * vtx];
            let i1 = mesh.normal_indices[3 * vtx + 1];
            let i2 = mesh.normal_indices[3 * vtx + 2];

            normals_indices.push([i0, i1, i2]);
        }

        if verts.len() > 0
        {
            //let material_arc;
            let material_arc: MaterialItem;

            //apply material
            if let Some(wavefront_mat_id) = mesh.material_id
            {
                let mut reusing_material = None;
                for mat in &double_check_materials
                {
                    if mat.0 == wavefront_mat_id
                    {
                        reusing_material = Some(mat.1.clone());
                        break;
                    }
                }

                if let Some(reusing_material) = reusing_material
                {
                    material_arc = reusing_material.clone();
                }
                else
                {
                    //let component_id = scene.id_manager.get_next_component_id();
                    let component_id = id_manager.write().unwrap().get_next_component_id();
                    material_arc = new_component!(Material::new(component_id, ""));

                    let mut material_guard = material_arc.write().unwrap();
                    let any = material_guard.as_any_mut();
                    let material = any.downcast_mut::<Material>().unwrap();

                    let mat: &tobj::Material = &wavefront_materials[wavefront_mat_id];

                    {
                        material.get_base_mut().name = mat.name.clone();
                    }

                    let material_data = material.get_data_mut().get_mut();

                    if mat.shininess.is_some()
                    {
                        material_data.shininess = mat.shininess.unwrap();
                    }

                    if mat.ambient.is_some()
                    {
                        let ambient = mat.ambient.unwrap();
                        material_data.ambient_color = Vector3::<f32>::new(ambient[0], ambient[1], ambient[2]);
                    }

                    if mat.specular.is_some()
                    {
                        let specular = mat.specular.unwrap();
                        material_data.specular_color = Vector3::<f32>::new(specular[0], specular[1], specular[2]);
                    }

                    if mat.diffuse.is_some()
                    {
                        let diffuse = mat.diffuse.unwrap();
                        material_data.base_color = Vector3::<f32>::new(diffuse[0], diffuse[1], diffuse[2]);
                    }

                    if mat.optical_density.is_some()
                    {
                        material_data.refraction_index = mat.optical_density.unwrap();
                    }

                    if mat.dissolve.is_some()
                    {
                        material_data.alpha = mat.dissolve.unwrap();
                    }


                    material_data.ambient_color = material_data.base_color * 0.01;

                    if let Some(illumination) = mat.illumination_model
                    {
                        if illumination > 2
                        {
                            material_data.reflectivity = 0.5;
                        }
                    }

                    // base texture
                    if mat.diffuse_texture.is_some()
                    {
                        println!("loading diffuse texture {}", mat.diffuse_texture.clone().unwrap());
                        let diffuse_texture = mat.diffuse_texture.clone().unwrap();
                        let tex_path = get_texture_path(&diffuse_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::Base);
                    }

                    // normal texture
                    if mat.normal_texture.is_some()
                    {
                        println!("loading normal texture {}", mat.normal_texture.clone().unwrap());
                        let normal_texture = mat.normal_texture.clone().unwrap();
                        let tex_path = get_texture_path(&normal_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::Normal);
                    }

                    // ambient texture
                    if mat.ambient_texture.is_some()
                    {
                        println!("loading ambient texture {}", mat.ambient_texture.clone().unwrap());
                        let ambient_texture = mat.ambient_texture.clone().unwrap();
                        let tex_path = get_texture_path(&ambient_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::AmbientEmissive);
                    }

                    // specular texture
                    if mat.specular_texture.is_some()
                    {
                        println!("loading specular texture {}", mat.specular_texture.clone().unwrap());
                        let specular_texture = mat.specular_texture.clone().unwrap();
                        let tex_path: String = get_texture_path(&specular_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::Specular);
                    }

                    // dissolve texture
                    if mat.dissolve_texture.is_some()
                    {
                        println!("loading dissolve texture {}", mat.dissolve_texture.clone().unwrap());
                        let dissolve_texture = mat.dissolve_texture.clone().unwrap();
                        let tex_path = get_texture_path(&dissolve_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::Alpha);
                    }

                    // shininess_texture
                    if mat.shininess_texture.is_some()
                    {
                        println!("loading shininess texture {}", mat.shininess_texture.clone().unwrap());
                        let shininess_texture = mat.shininess_texture.clone().unwrap();
                        let tex_path = get_texture_path(&shininess_texture, path);
                        let tex = load_texture_or_reuse(scene_id, main_queue.clone(), max_texture_resolution, tex_path.as_str(), None)?;
                        {
                            let mut tex = tex.write().unwrap();
                            let tex_data = tex.get_data_mut().get_mut();
                            tex_data.mipmapping = create_mipmaps;
                        }
                        material.set_texture(tex, TextureType::Shininess);
                    }

                    let material_arc_clone = material_arc.clone();
                    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
                    {
                        scene.add_material(component_id, &material_arc_clone);
                    }));
                    double_check_materials.push((wavefront_mat_id, material_arc.clone()));
                }
            }
            else
            {
                let material_id = id_manager.write().unwrap().get_next_component_id();
                material_arc = Arc::new(RwLock::new(Box::new(Material::new(material_id, ""))));
            }

            if uvs.len() > 0 && uv_indices.len() == 0
            {
                uv_indices = indices.clone();
            }

            if normals.len() > 0 && normals_indices.len() == 0
            {
                normals_indices = indices.clone();
            }

            let component_id = id_manager.write().unwrap().get_next_component_id();
            let item = Mesh::new_with_data(component_id, "mesh", verts, indices, uvs, uv_indices, normals, normals_indices);

            let id = id_manager.write().unwrap().get_next_node_id();
            loaded_ids.push(id);

            let node_arc = Node::new(id, m.name.as_str());
            {
                let mut node = node_arc.write().unwrap();
                node.add_component(Arc::new(RwLock::new(Box::new(item))));

                // add material
                node.add_component(material_arc);

                // add default instance
                //let node = scene.nodes.get_mut(0).unwrap();
                let instance_id = id_manager.write().unwrap().get_next_instance_id();
                node.create_default_instance(node_arc.clone(), instance_id);
            }

            scene_nodes.push(node_arc)
        }
    }

    // ********** add to scene **********
    let node_id = id_manager.write().unwrap().get_next_node_id();
    loaded_ids.push(node_id);

    let root_node = Node::new(node_id, resource_name.as_str());
    root_node.write().unwrap().root_node = true;

    let root_node_clone = root_node.clone();
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        scene.add_node(root_node_clone.clone());
    }));

    for scene_node in &scene_nodes
    {
        Node::add_node(root_node.clone(), scene_node.clone());
    }

    Ok(loaded_ids)
}