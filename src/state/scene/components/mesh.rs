use std::any::Any;

use nalgebra::{Point2, Point3, Isometry3, Vector3, Matrix4};
use parry3d::{shape::TriMesh, bounding_volume::Aabb};

use super::component::Component;

pub struct MeshData
{
    pub mesh: TriMesh,

    pub vertices: Vec<Point3<f32>>,
    pub indices: Vec<[u32; 3]>,

    pub uvs: Vec<Point2<f32>>,
    pub uv_indices: Vec<[u32; 3]>,

    pub normals: Vec<Point3<f32>>,
    pub normals_indices: Vec<[u32; 3]>,

    pub flip_normals: bool,
    pub b_box: Aabb,
}

pub struct Mesh
{
    data: MeshData,
}

impl Mesh
{
    pub fn new_with_data(vertices: Vec<Point3<f32>>, indices: Vec<[u32; 3]>, uvs: Vec<Point2<f32>>, uv_indices: Vec<[u32; 3]>, normals: Vec<Point3<f32>>, normals_indices: Vec<[u32; 3]>) -> Mesh
    {
        let mut mesh_data = MeshData
        {
            mesh: TriMesh::new(vertices.clone(), indices.clone()),

            vertices: vertices,
            indices: indices,
            uvs: uvs,
            uv_indices: uv_indices,
            normals: normals,
            normals_indices: normals_indices,

            flip_normals: false,
            b_box: Aabb::new_invalid(),
        };

        let mut mesh = Mesh { data: mesh_data };

        mesh.calc_bbox();

        mesh
    }

    pub fn new_plane(x0: Point3<f32>, x1: Point3<f32>, x2: Point3<f32>, x3: Point3<f32>) -> Mesh
    {
        let points = vec![ x0, x1, x2, x3, ];

        let uvs = vec!
        [
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];

        let indices = vec![[0u32, 1, 2], [0, 2, 3]];
        let uv_indices = vec![[0u32, 1, 2], [0, 2, 3]];

        let mut mesh = Mesh::new_with_data(points, indices, uvs, uv_indices, vec![], vec![]);

        mesh.calc_bbox();

        mesh
    }

    pub fn get_data(&self) -> &MeshData
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut MeshData
    {
        &mut self.data
    }

    fn calc_bbox(&mut self)
    {
        let trans = Isometry3::<f32>::identity();
        self.data.b_box = self.data.mesh.aabb(&trans);
    }

    pub fn get_normal(&self, hit: Point3<f32>, face_id: u32, tran_inverse: &Matrix4<f32>) -> Vector3<f32>
    {
        // https://stackoverflow.com/questions/23980748/triangle-texture-mapping-with-barycentric-coordinates
        // https://answers.unity.com/questions/383804/calculate-uv-coordinates-of-3d-point-on-plane-of-m.html

        //transform hit to local coords
        let hit_pos_local = tran_inverse * hit.to_homogeneous();
        let hit_pos_local = Point3::<f32>::from_homogeneous(hit_pos_local).unwrap();

        let f_id = (face_id % self.data.mesh.indices().len() as u32) as usize;

        let face = self.data.mesh.indices()[f_id];
        let normal_face = self.data.normals_indices[f_id];

        let i0 = face[0] as usize;
        let i1 = face[1] as usize;
        let i2 = face[2] as usize;

        let i_normal_0 = normal_face[0] as usize;
        let i_normal_1 = normal_face[1] as usize;
        let i_normal_2 = normal_face[2] as usize;

        let a = self.data.mesh.vertices()[i0].to_homogeneous();
        let b = self.data.mesh.vertices()[i1].to_homogeneous();
        let c = self.data.mesh.vertices()[i2].to_homogeneous();

        let a_t = self.data.normals[i_normal_0];
        let b_t = self.data.normals[i_normal_1];
        let c_t = self.data.normals[i_normal_2];

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
    fn is_enabled(&self) -> bool
    {
        true
    }

    fn component_name(&self) -> &'static str
    {
        "Mesh"
    }

    fn update(&mut self, time_delta: f32)
    {
        // TODO
    }

    fn as_any(&self) -> &dyn Any
    {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any
    {
        self
    }
}