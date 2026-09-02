#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use bytemuck::cast_slice;
use colored::Colorize;
use nalgebra::{Matrix4, Point2, Point3, Vector3};
use parry3d::{bounding_volume::{Aabb, BoundingSphere}, math::{Pose3, Vec3}, shape::TriMesh};
use serde::{Deserialize, Serialize};

use crate::{console_error, helper::{self, asset_path_descriptor::AssetPathDesciptor, change_tracker::ChangeTracker, generic::{point2_as_array, point3_as_array, vec3_as_array}, math::calculate_normal}, state::{helper::render_item::RenderItemOption, scene::{manager::id_manager, utilities::tags::Tags}}};

pub type MeshResourceItem = Arc<RwLock<Box<MeshResource>>>;

pub const JOINTS_LIMIT: usize = 4;

fn default_tri_mesh() -> TriMesh
{
    TriMesh::new(vec![], vec![]).unwrap()
}

fn default_aabb() -> Aabb
{
    Aabb::new_invalid()
}

fn default_sphere() -> BoundingSphere
{
    BoundingSphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
}

#[derive(Serialize, Deserialize)]
pub struct MeshResourceData
{
    #[serde(skip, default = "default_tri_mesh")]
    pub mesh: TriMesh,

    #[serde(skip, default)]
    pub vertices: Vec<Point3<f32>>,

    #[serde(skip, default)]
    pub indices: Vec<[u32; 3]>,

    #[serde(skip, default)]
    pub uvs_0: Vec<Point2<f32>>,
    #[serde(skip, default)]
    pub uvs_1: Vec<Point2<f32>>,
    #[serde(skip, default)]
    pub uvs_2: Vec<Point2<f32>>,
    #[serde(skip, default)]
    pub uvs_3: Vec<Point2<f32>>,
    #[serde(skip, default)]
    pub uv_indices: Vec<[u32; 3]>,

    #[serde(skip, default)]
    pub normals: Vec<Vector3<f32>>,
    #[serde(skip, default)]
    pub normals_indices: Vec<[u32; 3]>,

    #[serde(skip, default)]
    pub joints: Vec<[u32; JOINTS_LIMIT]>,
    #[serde(skip, default)]
    pub weights: Vec<[f32; JOINTS_LIMIT]>,

    #[serde(skip, default)]
    pub morph_target_positions: Vec<Vec<Point3<f32>>>,
    #[serde(skip, default)]
    pub morph_target_normals: Vec<Vec<Vector3<f32>>>,
    #[serde(skip, default)]
    pub morph_target_tangents: Vec<Vec<Vector3<f32>>>,

    #[serde(skip, default = "default_aabb")]
    pub b_box: Aabb,

    #[serde(skip, default = "default_sphere")]
    pub b_sphere: BoundingSphere
}

impl MeshResourceData
{
    pub fn clear(&mut self)
    {
        self.vertices.clear();
        self.indices.clear();

        self.uvs_0.clear();
        self.uvs_1.clear();
        self.uvs_2.clear();
        self.uvs_3.clear();
        self.uv_indices.clear();

        self.normals.clear();
        self.normals_indices.clear();

        self.joints.clear();
        self.weights.clear();

        self.morph_target_positions.clear();
        self.morph_target_normals.clear();
        self.morph_target_tangents.clear();

        // "empty" triangle
        let triangle = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)];
        let indices: [u32; 3] = [0, 1, 2];

        let mesh_res = TriMesh::new(triangle.to_vec(), [indices].to_vec());

        match mesh_res
        {
            Ok(mesh) => self.mesh = mesh,
            Err(e) =>
            {
                console_error!("{}", (format!("error loading mesh: {}", e)).red());
                self.mesh = TriMesh::new(vec![], vec![]).unwrap();
            }
        }

        self.b_box = Aabb::new_invalid();
    }
}

#[derive(Serialize, Deserialize)]
pub struct MeshResource
{
    #[serde(skip, default)]
    pub id: u32,
    pub uuid: String,
    pub source: Option<AssetPathDesciptor>,

    pub name: String,
    pub hash: String, // this is mainly used for initial loading and to check if there is a texture already loaded (in dynamic textires - this may does not get updates)
    pub tags: Tags,

    data: ChangeTracker<MeshResourceData>,

    #[serde(skip, default)]
    pub render_item: RenderItemOption,

    #[serde(skip, default)]
    pub delete_later_request: bool,
}

impl Default for MeshResource
{
    fn default() -> Self
    {
        Self
        {
            id: id_manager::get_next_mesh_id(),
            uuid: uuid::Uuid::new_v4().to_string(),
            source: None,

            name: "empty".to_string(),
            hash: "".to_string(),
            tags: Tags::new(),

            data: ChangeTracker::new(MeshResourceData
            {
                mesh: TriMesh::new(vec![], vec![]).unwrap(),

                vertices: vec![],
                indices: vec![],

                uvs_0: vec![],
                uvs_1: vec![],
                uvs_2: vec![],
                uvs_3: vec![],
                uv_indices: vec![],

                normals: vec![],
                normals_indices: vec![],

                joints: vec![],
                weights: vec![],

                morph_target_positions: vec![],
                morph_target_normals: vec![],
                morph_target_tangents: vec![],

                b_box: Aabb::new_invalid(),
                b_sphere: BoundingSphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
            }),

            render_item: None,

            delete_later_request: false
        }
    }
}

impl MeshResource
{
    pub fn new_with_data(name: &str, vertices: Vec<Point3<f32>>, indices: Vec<[u32; 3]>, uvs: Vec<Point2<f32>>, uv_indices: Vec<[u32; 3]>, normals: Vec<Vector3<f32>>, normals_indices: Vec<[u32; 3]>) -> MeshResource
    {
        let vertices_vec3: Vec<Vec3> = vertices.iter().map(|v| Vec3::new(v.x, v.y, v.z)).collect();
        let tri_mesh_res = TriMesh::new(vertices_vec3, indices.clone());

        let tri_mesh = match tri_mesh_res
        {
            Ok(tri_mesh) => tri_mesh,
            Err(e) =>
            {
                console_error!("{}", (format!("error loading mesh: {}", e)).red());
                TriMesh::new(vec![], vec![]).unwrap()
            }
        };

        let mut resource = MeshResource
        {
            id: id_manager::get_next_mesh_id(),
            uuid: uuid::Uuid::new_v4().to_string(),
            source: None,

            name: name.to_string(),
            tags: Tags::new(),
            hash: "".to_string(),

            data: ChangeTracker::new(MeshResourceData
            {
                mesh: tri_mesh,

                vertices: vertices,
                indices: indices,

                uvs_0: uvs,
                uvs_1: vec![],
                uvs_2: vec![],
                uvs_3: vec![],
                uv_indices: uv_indices,

                normals: normals,
                normals_indices: normals_indices,

                joints: vec![],
                weights: vec![],

                morph_target_positions: vec![],
                morph_target_normals: vec![],
                morph_target_tangents: vec![],

                b_box: Aabb::new_invalid(),
                b_sphere: BoundingSphere::new(Vec3::new(0.0, 0.0, 0.0), 0.0)
            }),

            render_item: None,

            delete_later_request: false
        };

        resource.calc_hash();
        resource.calc_bounding_volumes();

        // create normals if needed
        if resource.get_data().vertices.len() > 0 && resource.get_data().normals.len() == 0 && resource.get_data().indices.len() > 0
        {
            resource.create_normals();
        }

        resource
    }

    pub fn new_plane(name: &str, x0: Point3<f32>, x1: Point3<f32>, x2: Point3<f32>, x3: Point3<f32>) -> MeshResource
    {
        let points = vec![ x0, x1, x2, x3 ];

        let uvs = vec!
        [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];

        let indices = vec![[0u32, 1, 2], [0, 2, 3]];
        let uv_indices = vec![[0u32, 1, 2], [0, 2, 3]];

        let mut resource = MeshResource::new_with_data(name, points, indices, uvs, uv_indices, vec![], vec![]);

        resource.calc_bounding_volumes();

        // create normals if needed
        if resource.get_data().vertices.len() > 0 && resource.get_data().normals.len() == 0 && resource.get_data().indices.len() > 0
        {
            resource.create_normals();
        }

        resource
    }

    pub fn empty(name: &str) -> MeshResource
    {
        let mut resource = MeshResource::new_with_data(name, vec![], vec![], vec![], vec![], vec![], vec![]);

        resource.calc_bounding_volumes();

        resource
    }

    pub fn delete_later(&mut self)
    {
        self.delete_later_request = true;
    }

    pub fn get_data(&self) -> &MeshResourceData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MeshResourceData>
    {
        &mut self.data
    }

    pub fn create_normals(&mut self)
    {
        let mesh_data = self.get_data_mut().get_mut();

        mesh_data.normals.clear();
        mesh_data.normals_indices.clear();

        //for i in (0..mesh_data.vertices.len()).step_by(3)
        for face in &mesh_data.indices
        {
            let i0 = face[0];
            let i1 = face[1];
            let i2 = face[2];

            let v0 = mesh_data.vertices.get(i0 as usize).unwrap();
            let v1 = mesh_data.vertices.get(i1 as usize).unwrap();
            let v2 = mesh_data.vertices.get(i2 as usize).unwrap();

            let normal = calculate_normal(v0, v1, v2);
            mesh_data.normals.push(normal);
            mesh_data.normals.push(normal);
            mesh_data.normals.push(normal);

            mesh_data.normals_indices.push([i0, i1, i2]);
        }
    }

    pub fn flip_faces(&mut self)
    {
        {
            let data = self.get_data_mut().get_mut();

            // reverse the winding order [i0, i1, i2] --> [i0, i2, i1]
            for face in &mut data.indices         { face.swap(1, 2); }
            for face in &mut data.normals_indices { face.swap(1, 2); }
            for face in &mut data.uv_indices      { face.swap(1, 2); }

            // invert normals
            for n in &mut data.normals { *n = -*n; }

            // morph targets store deltas relative to the base data -> invert them too
            for morph in &mut data.morph_target_normals
            {
                for n in morph { *n = -*n; }
            }

            for morph in &mut data.morph_target_tangents
            {
                for t in morph { *t = -*t; }
            }

            // rebuild trimesh
            let vertices_vec3: Vec<Vec3> = data.vertices.iter().map(|v| Vec3::new(v.x, v.y, v.z)).collect();
            data.mesh = TriMesh::new(vertices_vec3, data.indices.clone()).unwrap();
        }

        self.calc_bounding_volumes();
    }

    pub fn calc_hash(&mut self)
    {
        let mesh_data = self.get_data();

        let mut bytes = Vec::new();

        for v in &mesh_data.vertices
        {
            bytes.extend_from_slice(cast_slice(&point3_as_array(v)));
        }

        for tri in &mesh_data.indices
        {
            bytes.extend_from_slice(cast_slice(tri));
        }

        for uv in &mesh_data.uvs_0
        {
            bytes.extend_from_slice(cast_slice(&point2_as_array(uv)));
        }

        for uv in &mesh_data.uvs_1
        {
            bytes.extend_from_slice(cast_slice(&point2_as_array(uv)));
        }

        for uv in &mesh_data.uvs_2
        {
            bytes.extend_from_slice(cast_slice(&point2_as_array(uv)));
        }

        for uv in &mesh_data.uvs_3
        {
            bytes.extend_from_slice(cast_slice(&point2_as_array(uv)));
        }

        for n in &mesh_data.normals
        {
            bytes.extend_from_slice(cast_slice(&vec3_as_array(n)));
        }

        for tri in &mesh_data.normals_indices
        {
            bytes.extend_from_slice(cast_slice(tri));
        }

        for joint in &mesh_data.joints
        {
            bytes.extend_from_slice(cast_slice(joint));
        }

        for weight in &mesh_data.weights
        {
            bytes.extend_from_slice(cast_slice(weight));
        }

        for morph in &mesh_data.morph_target_positions
        {
            for p in morph
            {
                bytes.extend_from_slice(cast_slice(&point3_as_array(p)));
            }
        }

        for morph in &mesh_data.morph_target_normals
        {
            for n in morph
            {
                bytes.extend_from_slice(cast_slice(&vec3_as_array(n)));
            }
        }

        for morph in &mesh_data.morph_target_tangents
        {
            for t in morph
            {
                bytes.extend_from_slice(cast_slice(&vec3_as_array(t)));
            }
        }

        self.hash = helper::crypto::get_hash_from_byte_vec(&bytes)
    }

    fn calc_bounding_volumes(&mut self)
    {
        let trans = Pose3::identity();
        let data = self.data.get_mut();
        data.b_box = data.mesh.aabb(&trans);
        data.b_sphere = data.mesh.bounding_sphere(&trans);
    }

    fn apply_transform(&mut self, transform: &Matrix4<f32>)
    {
        let data = self.data.get_mut();

        for v in &mut data.vertices
        {
            let new_pos = transform * v.to_homogeneous();
            v.x = new_pos.x;
            v.y = new_pos.y;
            v.z = new_pos.z;
        }

        for n in &mut data.normals
        {
            let new_vec = transform * n.to_homogeneous();
            n.x = new_vec.x;
            n.y = new_vec.y;
            n.z = new_vec.z;
        }

        // clear trimesh and rebuild
        let vertices_vec3: Vec<Vec3> = data.vertices.iter().map(|v| Vec3::new(v.x, v.y, v.z)).collect();
        data.mesh = TriMesh::new(vertices_vec3, data.indices.clone()).unwrap();

        self.calc_bounding_volumes();
    }

    pub fn merge(&mut self, mesh_data: &MeshResourceData)
    {
        let data = self.data.get_mut();

        let vertices_offset = data.vertices.len() as u32;
        let normals_offset = data.normals.len() as u32;
        let uv_offset = data.uvs_0.len() as u32;

        // vertices and indices
        data.vertices.extend(&mesh_data.vertices);

        for i in &mesh_data.indices
        {
            let i0 = i[0] + vertices_offset;
            let i1 = i[1] + vertices_offset;
            let i2 = i[2] + vertices_offset;
            data.indices.push([i0, i1, i2]);
        }

        // uvs and uv indices (1)
        data.uvs_0.extend(&mesh_data.uvs_0);
        data.uvs_1.extend(&mesh_data.uvs_1);
        data.uvs_2.extend(&mesh_data.uvs_2);
        data.uvs_3.extend(&mesh_data.uvs_3);

        for i in &mesh_data.uv_indices
        {
            let i0 = i[0] + uv_offset;
            let i1 = i[1] + uv_offset;
            let i2 = i[2] + uv_offset;
            data.uv_indices.push([i0, i1, i2]);
        }

        // normals
        data.normals.extend(&mesh_data.normals);

        for i in &mesh_data.normals_indices
        {
            let i0 = i[0] + normals_offset;
            let i1 = i[1] + normals_offset;
            let i2 = i[2] + normals_offset;
            data.normals_indices.push([i0, i1, i2]);
        }

        let vertices_vec3: Vec<Vec3> = data.vertices.iter().map(|v| Vec3::new(v.x, v.y, v.z)).collect();
        let mesh_res = TriMesh::new(vertices_vec3, data.indices.clone());
        let mesh = match mesh_res
        {
            Ok(mesh) => mesh,
            Err(e) =>
            {
                console_error!("{}", (format!("error loading mesh: {}", e)).red());
                TriMesh::new(vec![], vec![]).unwrap()
            }
        };

        data.mesh = mesh;

        self.calc_bounding_volumes();
    }

    pub fn merge_by_transformations(&mut self, transformations: &Vec::<Matrix4<f32>>)
    {
        let cloned_vertices;
        let cloned_indices;

        let cloned_uvs_1;
        let cloned_uvs_2;
        let cloned_uvs_3;
        let cloned_uvs_4;
        let cloned_uv_indices;

        let cloned_normals;
        let cloned_normals_indices;

        {
            let data = self.get_data();

            cloned_vertices = data.vertices.clone();
            cloned_indices = data.indices.clone();

            cloned_uvs_1 = data.uvs_0.clone();
            cloned_uvs_2 = data.uvs_1.clone();
            cloned_uvs_3 = data.uvs_2.clone();
            cloned_uvs_4 = data.uvs_3.clone();
            cloned_uv_indices = data.uv_indices.clone();

            cloned_normals = data.normals.clone();
            cloned_normals_indices = data.indices.clone();
        }

        {
            // clear data first
            let data = self.get_data_mut().get_mut();
            data.clear();

            // add by transformation
            for transform in transformations
            {
                let mut transformed_verts: Vec<Point3<f32>> = vec![];
                let mut transformed_normals: Vec<Vector3<f32>> = vec![];

                let vertices_offset = data.vertices.len() as u32;
                let normals_offset: u32 = data.normals.len() as u32;
                let uv_offset = data.uvs_0.len() as u32;

                for vertex in &cloned_vertices
                {
                    let new_pos = transform * vertex.to_homogeneous();
                    transformed_verts.push(new_pos.xyz().into());
                }

                for normal in &cloned_normals
                {
                    let new_normal = transform * normal.to_homogeneous();
                    transformed_normals.push(new_normal.xyz().into());
                }

                data.vertices.extend(&transformed_verts);
                data.normals.extend(&transformed_normals);

                for i in &cloned_indices
                {
                    let i0 = i[0] + vertices_offset;
                    let i1 = i[1] + vertices_offset;
                    let i2 = i[2] + vertices_offset;
                    data.indices.push([i0, i1, i2]);
                }

                data.uvs_0.extend(&cloned_uvs_1);
                data.uvs_1.extend(&cloned_uvs_2);
                data.uvs_2.extend(&cloned_uvs_3);
                data.uvs_3.extend(&cloned_uvs_4);

                for i in &cloned_uv_indices
                {
                    let i0 = i[0] + uv_offset;
                    let i1 = i[1] + uv_offset;
                    let i2 = i[2] + uv_offset;
                    data.uv_indices.push([i0, i1, i2]);
                }

                for i in &cloned_normals_indices
                {
                    let i0 = i[0] + normals_offset;
                    let i1 = i[1] + normals_offset;
                    let i2 = i[2] + normals_offset;
                    data.normals_indices.push([i0, i1, i2]);
                }
            }

            // create mesh
            let vertices_vec3: Vec<Vec3> = data.vertices.iter().map(|v| Vec3::new(v.x, v.y, v.z)).collect();
            let mesh_res = TriMesh::new(vertices_vec3, data.indices.clone());
            let mesh = match mesh_res
            {
                Ok(mesh) => mesh,
                Err(e) =>
                {
                    console_error!("{}", (format!("error loading mesh: {}", e)).red());
                    TriMesh::new(vec![], vec![]).unwrap()
                }
            };
            data.mesh = mesh;
        }

        self.calc_bounding_volumes();
    }

    pub fn get_normal(&self, hit: Point3<f32>, face_id: u32, tran_inverse: &Matrix4<f32>, vertices: &Vec<Point3<f32>>) -> Vector3<f32>
    {
        let data = self.data.get_ref();

        // https://stackoverflow.com/questions/23980748/triangle-texture-mapping-with-barycentric-coordinates
        // https://answers.unity.com/questions/383804/calculate-uv-coordinates-of-3d-point-on-plane-of-m.html

        //transform hit to local coords
        let hit_pos_local = tran_inverse * hit.to_homogeneous();
        let hit_pos_local = Point3::<f32>::from_homogeneous(hit_pos_local).unwrap();

        let f_id = (face_id % data.mesh.indices().len() as u32) as usize;

        let face = data.mesh.indices()[f_id];
        let normal_face = data.normals_indices[f_id];

        let i0 = face[0] as usize;
        let i1 = face[1] as usize;
        let i2 = face[2] as usize;

        let i_normal_0 = normal_face[0] as usize;
        let i_normal_1 = normal_face[1] as usize;
        let i_normal_2 = normal_face[2] as usize;

        let a = vertices[i0].to_homogeneous();
        let b = vertices[i1].to_homogeneous();
        let c = vertices[i2].to_homogeneous();

        let a_t = data.normals[i_normal_0];
        let b_t = data.normals[i_normal_1];
        let c_t = data.normals[i_normal_2];

        let a = Point3::<f32>::from_homogeneous(a).unwrap();
        let b = Point3::<f32>::from_homogeneous(b).unwrap();
        let c = Point3::<f32>::from_homogeneous(c).unwrap();

        let f1 = a - hit_pos_local;
        let f2 = b - hit_pos_local;
        let f3 = c - hit_pos_local;

        let a = (a-b).cross(&(a-c)).magnitude();
        let a1 = f2.cross(&f3).magnitude() / a;
        let a2 = f3.cross(&f1).magnitude() / a;
        let a3 = f1.cross(&f2).magnitude() / a;

        let part_1 = a_t * a1;
        let part_2 = b_t * a2;
        let part_3 = c_t * a3;

        let normal = Point3::<f32>::new
        (
            part_1.x + part_2.x + part_3.x,
            part_1.y + part_2.y + part_3.y,
            part_1.z + part_2.z + part_3.z,
        );

        Vector3::<f32>::new(normal.x, normal.y, normal.z)
    }

    pub fn ui_info(&self, ui: &mut egui::Ui)
    {
        let data = self.get_data();

        ui.label(format!(" ⚫ hash: {}", self.hash));

        ui.label(format!(" ⚫ vertices: {}", data.vertices.len()));
        ui.label(format!(" ⚫ indices: {}", data.indices.len()));

        ui.label(format!(" ⚫ uvs_0: {}", data.uvs_0.len()));
        ui.label(format!(" ⚫ uvs_1: {}", data.uvs_1.len()));
        ui.label(format!(" ⚫ uvs_2: {}", data.uvs_2.len()));
        ui.label(format!(" ⚫ uvs_3: {}", data.uvs_3.len()));
        ui.label(format!(" ⚫ uv_indices: {}", data.uv_indices.len()));

        ui.label(format!(" ⚫ normals: {}", data.normals.len()));
        ui.label(format!(" ⚫ normals_indices: {}", data.normals_indices.len()));

        ui.label(format!(" ⚫ joints: {}", data.joints.len()));
        ui.label(format!(" ⚫ weights: {}", data.weights.len()));

        ui.label(format!(" ⚫ morph target positions: {}", data.morph_target_positions.len()));
        ui.label(format!(" ⚫ morph target normals: {}", data.morph_target_normals.len()));
        ui.label(format!(" ⚫ morph target tangents: {}", data.morph_target_tangents.len()));

        ui.label(format!(" ⚫ bbox min: [{:.3}, {:.3}, {:.3}]", data.b_box.mins.x, data.b_box.mins.z, data.b_box.mins.z));
        ui.label(format!(" ⚫ bbox max: [{:.3}, {:.3}, {:.3}]", data.b_box.maxs.x, data.b_box.maxs.z, data.b_box.maxs.z));

        ui.label(format!(" ⚫ b sphere: [{:.3}, {:.3}, {:.3}] r={:.3}", data.b_sphere.center.x, data.b_sphere.center.y, data.b_sphere.center.z, data.b_sphere.radius));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui|
        {
            if ui.button("Flip Faces").on_hover_text("Flip all faces ot the mesh").clicked()
            {
                self.flip_faces();
            };
        });
    }
}