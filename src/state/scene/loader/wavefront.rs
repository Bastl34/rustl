use std::io::{Cursor, BufReader};

use nalgebra::{Point3, Point2};

use crate::{resources::resources::load_string_async, state::scene::{components::{mesh::Mesh, self}, scene::Scene, node::Node}, helper};

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
            /*
            let material_arc;

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
                    material_arc = self.get_material_by_id(reusing_material_object_id).unwrap();
                }
                else
                {
                    let material_id = self.get_next_id();
                    material_arc = Arc::new(RwLock::new(Box::new(Material::new(material_id, ""))));

                    let mat: &tobj::Material = &wavefront_materials[wavefront_mat_id];
                    let mut material = material_arc.write().unwrap();
                    material.name = mat.name.clone();

                    material.shininess = mat.shininess;
                    material.ambient_color = Vector3::<f32>::new(mat.ambient[0], mat.ambient[1], mat.ambient[2]);
                    material.specular_color = Vector3::<f32>::new(mat.specular[0], mat.specular[1], mat.specular[2]);
                    material.base_color = Vector3::<f32>::new(mat.diffuse[0], mat.diffuse[1], mat.diffuse[2]);
                    material.refraction_index = mat.optical_density;
                    material.alpha = mat.dissolve;

                    material.ambient_color = material.base_color * 0.01;

                    if let Some(illumination) = mat.illumination_model
                    {
                        if illumination > 2
                        {
                            material.reflectivity = 0.5;
                        }
                    }

                    // base texture
                    if !mat.diffuse_texture.is_empty()
                    {
                        let tex_path = self.get_texture_path(&mat.diffuse_texture, path);
                        dbg!(&tex_path);
                        material.load_texture(&tex_path, TextureType::Base);
                    }

                    // normal texture
                    if !mat.normal_texture.is_empty()
                    {
                        let tex_path = self.get_texture_path(&mat.normal_texture, path);
                        dbg!(&tex_path);
                        material.load_texture(&tex_path, TextureType::Normal);
                    }

                    // ambient texture
                    if !mat.ambient_texture.is_empty()
                    {
                        let tex_path = self.get_texture_path(&mat.ambient_texture, path);
                        dbg!(&tex_path);
                        material.load_texture(&tex_path, TextureType::AmbientEmissive);
                    }

                    // specular texture
                    if !mat.specular_texture.is_empty()
                    {
                        let tex_path = self.get_texture_path(&mat.specular_texture, path);
                        dbg!(&tex_path);
                        material.load_texture(&tex_path, TextureType::Specular);
                    }

                    // specular texture
                    if !mat.dissolve_texture.is_empty()
                    {
                        let tex_path = self.get_texture_path(&mat.dissolve_texture, path);
                        dbg!(&tex_path);
                        material.load_texture(&tex_path, TextureType::Alpha);
                    }

                    // shininess_texture is not supported

                    self.materials.push(material_arc.clone());
                    double_check_materials.push((wavefront_mat_id, material_id));
                }
            }
            else
            {
                let material_id = self.get_next_id();
                material_arc = Arc::new(RwLock::new(Box::new(Material::new(material_id, ""))));
            }
            */

            //let mut item = Mesh::new_with_data(m.name.as_str(), material_arc.clone(), verts, indices, uvs, uv_indices, normals, normals_indices);
            let item = Mesh::new_with_data(verts, indices, uvs, uv_indices, normals, normals_indices);

            let id = scene.id_manager.get_next_node_id();
            loaded_ids.push(id);

            let mut node = Node::new(id, m.name.as_str(), None);
            node.add_component(Box::new(item));

            scene.nodes.push(Box::new(node));
        }

    }
    Ok(loaded_ids)
}