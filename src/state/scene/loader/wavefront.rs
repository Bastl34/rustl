use std::{io::{Cursor, BufReader}, sync::{RwLock, Arc}, path::Path};

use nalgebra::{Point3, Point2, Vector3};

use crate::{console_error, console_log, helper::{self, asset_path_descriptor::AssetPathDesciptor, file::get_stem, option_or_id::OptionOrId}, new_component, resources::resources::{load_binary, load_string}, state::{resources::{mesh_resource::{MeshResource}, texture::TextureItem, utilities::resource_utils::load_texture_byte}, scene::{components::{component::Component, material::{Material, MaterialItem, TextureType}, mesh::Mesh}, loader::{asset_container::AssetContainer, loader::LoaderOptions}, node::Node, scene::Scene}}};

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

fn load_texture_with_cache(options: &LoaderOptions, path: &str) -> anyhow::Result<TextureItem>
{
    let image_bytes = load_binary(path)?;
    let hash = crate::helper::crypto::get_hash_from_byte_vec(&image_bytes);

    if let Some(cache) = &options.texture_cache
    {
        if let Some(cached) = cache.get(&hash).cloned()
        {
            console_log!("reusing cached texture {}", path);
            return Ok(cached);
        }
    }

    let name = helper::file::get_stem(path);
    let tex = load_texture_byte(options.max_texture_resolution, options.create_mipmaps, &image_bytes, name.as_str(), path, None);
    let _ = hash;
    Ok(tex)
}

pub fn load(options: &LoaderOptions) -> anyhow::Result<AssetContainer>
{
    let mut asset_container = AssetContainer::new(options.clone());

    let resource_name = get_stem(options.path.as_str());

    let obj_text = load_string(options.path.as_str())?;
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
                file_path = helper::file::get_dirname(options.path.as_str()) + "/" + &file_path;
            }

            let mat_text = load_string(&file_path).unwrap();
            tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(mat_text)))
        },
    )?;

    let wavefront_materials = materials.unwrap();
    let mut scene_nodes = vec![];

    let mut double_check_materials: Vec<(usize, MaterialItem)> = vec![];

    for (i, m) in models.iter().enumerate()
    {
        let mesh = &m.mesh;

        if mesh.texcoord_indices.len() > 0 && mesh.indices.len() != mesh.texcoord_indices.len()
        {
            console_error!("Error can not load {}, because of indices mismatch", m.name.as_str());
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

                if reusing_material.is_none() && options.reuse_materials
                {
                    let material_name = &wavefront_materials[wavefront_mat_id].name;
                    if !material_name.is_empty()
                    {
                        if let Some(cache) = &options.material_cache
                        {
                            if let Some(cached) = cache.get(material_name).cloned()
                            {
                                console_log!("reusing cached material {}", material_name);
                                asset_container.materials.push(cached.clone());
                                double_check_materials.push((wavefront_mat_id, cached.clone()));
                                reusing_material = Some(cached);
                            }
                        }
                    }
                }

                if let Some(reusing_material) = reusing_material
                {
                    material_arc = reusing_material.clone();
                }
                else
                {
                    material_arc = new_component!(Material::new(""));

                    {
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
                            console_log!("loading diffuse texture {}", mat.diffuse_texture.clone().unwrap());
                            let diffuse_texture = mat.diffuse_texture.clone().unwrap();
                            let tex_path = get_texture_path(&diffuse_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::Base);
                        }

                        // normal texture
                        if mat.normal_texture.is_some()
                        {
                            console_log!("loading normal texture {}", mat.normal_texture.clone().unwrap());
                            let normal_texture = mat.normal_texture.clone().unwrap();
                            let tex_path = get_texture_path(&normal_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::Normal);
                        }

                        // ambient texture
                        if mat.ambient_texture.is_some()
                        {
                            console_log!("loading ambient texture {}", mat.ambient_texture.clone().unwrap());
                            let ambient_texture = mat.ambient_texture.clone().unwrap();
                            let tex_path = get_texture_path(&ambient_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::AmbientEmissive);
                        }

                        // specular texture
                        if mat.specular_texture.is_some()
                        {
                            console_log!("loading specular texture {}", mat.specular_texture.clone().unwrap());
                            let specular_texture = mat.specular_texture.clone().unwrap();
                            let tex_path: String = get_texture_path(&specular_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::Specular);
                        }

                        // dissolve texture
                        if mat.dissolve_texture.is_some()
                        {
                            console_log!("loading dissolve texture {}", mat.dissolve_texture.clone().unwrap());
                            let dissolve_texture = mat.dissolve_texture.clone().unwrap();
                            let tex_path = get_texture_path(&dissolve_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::Alpha);
                        }

                        // shininess_texture
                        if mat.shininess_texture.is_some()
                        {
                            console_log!("loading shininess texture {}", mat.shininess_texture.clone().unwrap());
                            let shininess_texture = mat.shininess_texture.clone().unwrap();
                            let tex_path = get_texture_path(&shininess_texture, options.path.as_str());
                            let tex = load_texture_with_cache(options, tex_path.as_str())?;
                            asset_container.textures.push(tex.clone());
                            material.set_texture(tex, TextureType::Shininess);
                        }
                    }

                    asset_container.materials.push(material_arc.clone());
                    double_check_materials.push((wavefront_mat_id, material_arc.clone()));
                }
            }
            else
            {
                material_arc = Arc::new(RwLock::new(Box::new(Material::new(""))));
            }

            if uvs.len() > 0 && uv_indices.len() == 0
            {
                uv_indices = indices.clone();
            }

            if normals.len() > 0 && normals_indices.len() == 0
            {
                normals_indices = indices.clone();
            }

            //let mesh_component = Mesh::new_with_data("mesh", verts, indices, uvs, uv_indices, normals, normals_indices);

            let mut mesh_resource: MeshResource = MeshResource::new_with_data("Mesh", verts, indices, uvs, uv_indices, normals, normals_indices);
            mesh_resource.source = Some(AssetPathDesciptor::new_from_path(options.path.to_string()));
            mesh_resource.source.as_mut().unwrap().inner_path = format!("#Primitive{}", i);

            let mesh_resource_result = Arc::new(RwLock::new(Box::new(mesh_resource)));
            asset_container.mesh_resources.push(mesh_resource_result.clone());

            let mut mesh_component: Mesh = Mesh::new("Mesh");
            mesh_component.mesh_resource = OptionOrId::Some(mesh_resource_result);

            let node_arc = Node::new(m.name.as_str());
            asset_container.nodes.push(node_arc.clone());

            {
                let mut node = node_arc.write().unwrap();
                node.add_component(Arc::new(RwLock::new(Box::new(mesh_component))));

                // add material
                node.add_component(material_arc);

                // add default instance
                //let node = scene.nodes.get_mut(0).unwrap();
                node.create_default_instance(node_arc.clone());
            }

            scene_nodes.push(node_arc)
        }
    }

    let root_node = Node::new(resource_name.as_str());

    root_node.write().unwrap().root_node = true;
    root_node.write().unwrap().source = Some(AssetPathDesciptor::new_from_path(options.path.to_string()));

    asset_container.nodes.insert(0, root_node.clone());
    asset_container.root_nodes.push(root_node.clone());

    // ********** add all to root node **********
    for scene_node in &scene_nodes
    {
        Node::add_node(root_node.clone(), scene_node.clone());
    }

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
    /*
    if hide_root_node
    {
        root_node.write().unwrap().settings.visible = false;
    }

    let root_node_clone = root_node.clone();
    execute_on_scene_mut_and_wait(main_queue.clone(), scene_id, Box::new(move |scene: &mut Scene|
    {
        if let Some(parent_node_id) = parent_node_id
        {
            let parent_node = scene.find_node_by_id(parent_node_id);
            if let Some(parent_node) = parent_node
            {
                Node::add_node(parent_node.clone(), root_node_clone.clone());
            }
            else
            {
                console_error!("can not find parent node by id");
            }
        }
        else
        {
            scene.add_node(root_node_clone.clone());
        }
    }));

    Ok(loaded_ids)
    */

    Ok(asset_container)
}