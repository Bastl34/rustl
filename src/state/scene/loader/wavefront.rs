use std::{io::{Cursor, BufReader}, sync::{RwLock, Arc}, path::Path, ops::Deref};

use nalgebra::{Point3, Point2, Vector3};

use crate::{resources::resources::load_string_async, state::scene::{components::{mesh::Mesh, self, material::{Material, TextureType, MaterialItem}, component::Component}, scene::Scene, node::Node}, helper, new_shared_component, shared_component_downcast_mut};

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

pub async fn load(path: &str, scene: &mut Scene) -> anyhow::Result<Vec<u32>>
{
    let mut loaded_ids: Vec<u32> = vec![];

    let obj_text = load_string_async(path).await?;
    let obj_cursor = Cursor::new(obj_text);
    let mut obj_reader = BufReader::new(obj_cursor);

    let (models, materials) = tobj::load_obj_buf_async
    (
        &mut obj_reader,
        &tobj::LoadOptions
        {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
        |p| async move
        {
            let mut file_path = p;
            if !helper::file::is_absolute(file_path.as_str())
            {
                file_path = helper::file::get_dirname(path) + "/" + &file_path;
            }

            let mat_text = load_string_async(&file_path).await.unwrap();
            tobj::load_mtl_buf(&mut BufReader::new(Cursor::new(mat_text)))
        },
    )
    .await?;


    let wavefront_materials = materials.unwrap();

    let mut double_check_materials: Vec<(usize, u32)> = vec![];

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
        let mut normals: Vec<Point3<f32>> = vec![];

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

            normals.push(Point3::<f32>::new(x, y, z));
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
                let mut reusing_material_object_id = 0;
                for mat in &double_check_materials
                {
                    if mat.0 == wavefront_mat_id
                    {
                        reusing_material_object_id = mat.1;
                        break;
                    }
                }

                if reusing_material_object_id != 0
                {
                    material_arc = scene.get_material_by_id(reusing_material_object_id).unwrap();
                }
                else
                {
                    let material_id = scene.id_manager.get_next_material_id();
                    material_arc = new_shared_component!(Material::new(material_id, ""));

                    let mut material_guard = material_arc.write().unwrap();
                    let any = material_guard.as_any_mut();
                    let mut material =  shared_component_downcast_mut!(any, Material);

                    let mat: &tobj::Material = &wavefront_materials[wavefront_mat_id];

                    material.name = mat.name.clone();

                    if mat.shininess.is_some()
                    {
                        material.shininess = mat.shininess.unwrap();
                    }

                    if mat.ambient.is_some()
                    {
                        let ambient = mat.ambient.unwrap();
                        material.ambient_color = Vector3::<f32>::new(ambient[0], ambient[1], ambient[2]);
                    }

                    if mat.specular.is_some()
                    {
                        let specular = mat.specular.unwrap();
                        material.specular_color = Vector3::<f32>::new(specular[0], specular[1], specular[2]);
                    }

                    if mat.diffuse.is_some()
                    {
                        let diffuse = mat.diffuse.unwrap();
                        material.base_color = Vector3::<f32>::new(diffuse[0], diffuse[1], diffuse[2]);
                    }

                    if mat.optical_density.is_some()
                    {
                        material.refraction_index = mat.optical_density.unwrap();
                    }

                    if mat.dissolve.is_some()
                    {
                        material.alpha = mat.dissolve.unwrap();
                    }


                    material.ambient_color = material.base_color * 0.01;

                    if let Some(illumination) = mat.illumination_model
                    {
                        if illumination > 2
                        {
                            material.reflectivity = 0.5;
                        }
                    }

                    // base texture
                    if mat.diffuse_texture.is_some()
                    {
                        let diffuse_texture = mat.diffuse_texture.clone().unwrap();
                        let tex_path = get_texture_path(&diffuse_texture, path);
                        let tex = scene.load_texture_or_reuse(tex_path.as_str()).await?;
                        material.set_texture(tex, TextureType::Base);
                    }

                    // normal texture
                    if mat.normal_texture.is_some()
                    {
                        let normal_texture = mat.normal_texture.clone().unwrap();
                        let tex_path = get_texture_path(&normal_texture, path);
                        let tex = scene.load_texture_or_reuse(tex_path.as_str()).await?;
                        material.set_texture(tex, TextureType::Normal);
                    }

                    // ambient texture
                    if mat.ambient_texture.is_some()
                    {
                        let ambient_texture = mat.ambient_texture.clone().unwrap();
                        let tex_path = get_texture_path(&ambient_texture, path);
                        let tex = scene.load_texture_or_reuse(tex_path.as_str()).await?;
                        material.set_texture(tex, TextureType::AmbientEmissive);
                    }

                    // specular texture
                    if mat.specular_texture.is_some()
                    {
                        let specular_texture = mat.specular_texture.clone().unwrap();
                        let tex_path: String = get_texture_path(&specular_texture, path);
                        let tex = scene.load_texture_or_reuse(tex_path.as_str()).await?;
                        material.set_texture(tex, TextureType::Specular);
                    }

                    // specular texture
                    if mat.dissolve_texture.is_some()
                    {
                        let dissolve_texture = mat.dissolve_texture.clone().unwrap();
                        let tex_path = get_texture_path(&dissolve_texture, path);
                        let tex = scene.load_texture_or_reuse(tex_path.as_str()).await?;
                        material.set_texture(tex, TextureType::Alpha);
                    }

                    // shininess_texture is not supported

                    scene.add_material(material_id, &material_arc);
                    double_check_materials.push((wavefront_mat_id, material_id));
                }
            }
            else
            {
                let material_id = scene.id_manager.get_next_material_id();
                material_arc = Arc::new(RwLock::new(Box::new(Material::new(material_id, ""))));
            }

            //let mut item = Mesh::new_with_data(m.name.as_str(), material_arc.clone(), verts, indices, uvs, uv_indices, normals, normals_indices);
            let item = Mesh::new_with_data(verts, indices, uvs, uv_indices, normals, normals_indices);

            let id = scene.id_manager.get_next_node_id();
            loaded_ids.push(id);

            let mut node = Node::new(id, m.name.as_str());
            node.add_component(Box::new(item));

            // add material
            node.add_shared_component(material_arc);

            //node.add_component(material_arc.write().unwrap());

            scene.nodes.push(Box::new(node));

            dbg!("ooooooooooooookkk");
        }

    }
    Ok(loaded_ids)
}