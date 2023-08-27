use std::any::Any;

use nalgebra::{Point2, Point3, Isometry3, Vector3, Matrix4};
use parry3d::{shape::TriMesh, bounding_volume::Aabb};

use crate::{component_impl_default, helper::change_tracker::ChangeTracker, state::scene::node::NodeItem, component_impl_no_update};

use super::component::{Component, ComponentBase};

pub struct MeshData
{
    pub mesh: TriMesh,

    pub vertices: Vec<Point3<f32>>,
    pub indices: Vec<[u32; 3]>,

    pub uvs_1: Vec<Point2<f32>>,
    pub uvs_2: Vec<Point2<f32>>,
    pub uvs_3: Vec<Point2<f32>>,
    pub uv_indices: Vec<[u32; 3]>,

    pub normals: Vec<Point3<f32>>,
    pub normals_indices: Vec<[u32; 3]>,

    pub flip_normals: bool,
    pub b_box: Aabb,
}

pub struct Mesh
{
    base: ComponentBase,
    data: ChangeTracker<MeshData>,
}

impl Mesh
{
    pub fn new_with_data(id: u64, vertices: Vec<Point3<f32>>, indices: Vec<[u32; 3]>, uvs: Vec<Point2<f32>>, uv_indices: Vec<[u32; 3]>, normals: Vec<Point3<f32>>, normals_indices: Vec<[u32; 3]>) -> Mesh
    {
        let mesh_data = MeshData
        {
            mesh: TriMesh::new(vertices.clone(), indices.clone()),

            vertices: vertices,
            indices: indices,
            normals: normals,
            normals_indices: normals_indices,

            uvs_1: uvs,
            uvs_2: vec![],
            uvs_3: vec![],
            uv_indices: uv_indices,

            flip_normals: false,
            b_box: Aabb::new_invalid(),
        };

        let mut mesh = Mesh
        {
            base: ComponentBase::new(id, "Default".to_string(), "Mesh".to_string(), "◼".to_string()),
            data: ChangeTracker::new(mesh_data)
        };

        mesh.calc_bbox();

        mesh
    }

    pub fn new_plane(id: u64, x0: Point3<f32>, x1: Point3<f32>, x2: Point3<f32>, x3: Point3<f32>) -> Mesh
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

        let mut mesh = Mesh::new_with_data(id, points, indices, uvs, uv_indices, vec![], vec![]);

        mesh.calc_bbox();

        mesh
    }

    pub fn empty(id: u64) -> Mesh
    {
        let mut mesh = Mesh::new_with_data(id, vec![], vec![], vec![], vec![], vec![], vec![]);

        mesh.calc_bbox();

        mesh
    }

    pub fn get_data(&self) -> &MeshData
    {
        &self.data.get_ref()
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MeshData>
    {
        &mut self.data
    }

    fn calc_bbox(&mut self)
    {
        let trans = Isometry3::<f32>::identity();
        let mut data = self.data.get_mut();
        data.b_box = data.mesh.aabb(&trans);
    }

    fn apply_transform(&mut self, trasform: &Matrix4<f32>)
    {
        let mut data = self.data.get_mut();

        for v in &mut data.vertices
        {
            let new_pos = trasform * v.to_homogeneous();
            v.x = new_pos.x;
            v.y = new_pos.y;
            v.z = new_pos.z;
        }

        // clear trimesh and rebuild
        data.mesh = TriMesh::new(data.vertices.clone(), data.indices.clone());

        self.calc_bbox();
    }

    pub fn merge(&mut self, mesh_data: &MeshData)
    {
        let mut data = self.data.get_mut();

        // tri mesh
        data.mesh.append(&mesh_data.mesh);

        // vertices and indices
        data.vertices.extend(&mesh_data.vertices);

        let index_offset = data.indices.len() as u32;
        for i in &mesh_data.indices
        {
            let i0 = i[0] + index_offset;
            let i1 = i[1] + index_offset;
            let i2 = i[2] + index_offset;
            data.indices.push([i0, i1, i2]);
        }

        // uvs and uv indices (1)
        data.uvs_1.extend(&mesh_data.uvs_1);
        data.uvs_2.extend(&mesh_data.uvs_2);
        data.uvs_3.extend(&mesh_data.uvs_3);

        let uv_index_offset = data.uv_indices.len() as u32;
        for i in &mesh_data.uv_indices
        {
            let i0 = i[0] + uv_index_offset;
            let i1 = i[1] + uv_index_offset;
            let i2 = i[2] + uv_index_offset;
            data.uv_indices.push([i0, i1, i2]);
        }

        // normals
        data.normals.extend(&mesh_data.normals);

        let normal_index_offset = data.normals_indices.len() as u32;
        for i in &mesh_data.normals_indices
        {
            let i0 = i[0] + normal_index_offset;
            let i1 = i[1] + normal_index_offset;
            let i2 = i[2] + normal_index_offset;
            data.normals_indices.push([i0, i1, i2]);
        }

        self.calc_bbox();
    }

    pub fn get_normal(&self, hit: Point3<f32>, face_id: u32, tran_inverse: &Matrix4<f32>) -> Vector3<f32>
    {
        let mut data = self.data.get_ref();

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

        let a = data.mesh.vertices()[i0].to_homogeneous();
        let b = data.mesh.vertices()[i1].to_homogeneous();
        let c = data.mesh.vertices()[i2].to_homogeneous();

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
}

impl Component for Mesh
{
    component_impl_default!();
    component_impl_no_update!();

    fn instantiable(&self) -> bool
    {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {

    }
}