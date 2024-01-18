use nalgebra::{Point2, Point3, Isometry3, Vector3, Matrix4};
use parry3d::{shape::{TriMesh, FeatureId}, bounding_volume::Aabb, query::{Ray, RayCast}};

use crate::{component_impl_default, helper::{change_tracker::ChangeTracker, math::calculate_normal}, state::{scene::{node::NodeItem, components::mesh}, helper::render_item::RenderItemOption}, component_impl_no_update, component_impl_set_enabled};

use super::component::{Component, ComponentBase};

pub const JOINTS_LIMIT: usize = 4;
pub const MOPRH_TARGETS_LIMIT: usize = 8;

pub struct MeshData
{
    pub mesh: TriMesh,

    pub vertices: Vec<Point3<f32>>,
    pub indices: Vec<[u32; 3]>,

    pub uvs_1: Vec<Point2<f32>>,
    pub uvs_2: Vec<Point2<f32>>,
    pub uvs_3: Vec<Point2<f32>>,
    pub uv_indices: Vec<[u32; 3]>,

    pub normals: Vec<Vector3<f32>>,
    pub normals_indices: Vec<[u32; 3]>,

    pub joints: Vec<[u32; JOINTS_LIMIT]>,
    pub weights: Vec<[f32; JOINTS_LIMIT]>,

    pub morph_target_positions: Vec<Vec<Point3<f32>>>,
    pub morph_target_normals: Vec<Vec<Vector3<f32>>>,
    pub morph_target_tangents: Vec<Vec<Vector3<f32>>>,

    pub flip_normals: bool,
    pub b_box: Aabb,
}

impl MeshData
{
    pub fn clear(&mut self)
    {
        self.vertices.clear();
        self.indices.clear();

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
        let triangle = [Point3::<f32>::new(0.0, 0.0, 0.0), Point3::<f32>::new(0.0, 0.0, 0.0), Point3::<f32>::new(0.0, 0.0, 0.0)];
        let indices: [u32; 3] = [0, 1, 2];

        self.mesh = TriMesh::new(triangle.to_vec(), [indices].to_vec());

        self.b_box = Aabb::new_invalid();
    }
}

pub struct Mesh
{
    base: ComponentBase,
    data: ChangeTracker<MeshData>,

    pub morph_target_render_item: RenderItemOption,
}

impl Mesh
{
    pub fn new_with_data(id: u64, name: &str, vertices: Vec<Point3<f32>>, indices: Vec<[u32; 3]>, uvs: Vec<Point2<f32>>, uv_indices: Vec<[u32; 3]>, normals: Vec<Vector3<f32>>, normals_indices: Vec<[u32; 3]>) -> Mesh
    {
        let mesh_data = MeshData
        {
            mesh: TriMesh::new(vertices.clone(), indices.clone()),

            vertices: vertices,
            indices: indices,

            uvs_1: uvs,
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

            flip_normals: false,
            b_box: Aabb::new_invalid(),
        };

        let mut mesh = Mesh
        {
            base: ComponentBase::new(id, name.to_string(), "Mesh".to_string(), "◼".to_string()),
            data: ChangeTracker::new(mesh_data),

            morph_target_render_item: None
        };

        mesh.calc_bbox();

        // create normals if needed
        if mesh.get_data().vertices.len() > 0 && mesh.get_data().normals.len() == 0 && mesh.get_data().indices.len() > 0
        {
            mesh.create_normals();
        }

        mesh
    }

    pub fn new_plane(id: u64, name: &str, x0: Point3<f32>, x1: Point3<f32>, x2: Point3<f32>, x3: Point3<f32>) -> Mesh
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

        let mut mesh = Mesh::new_with_data(id, name, points, indices, uvs, uv_indices, vec![], vec![]);

        mesh.calc_bbox();

        // create normals if needed
        if mesh.get_data().vertices.len() > 0 && mesh.get_data().normals.len() == 0 && mesh.get_data().indices.len() > 0
        {
            mesh.create_normals();
        }

        mesh
    }

    pub fn empty(id: u64, name: &str) -> Mesh
    {
        let mut mesh = Mesh::new_with_data(id, name, vec![], vec![], vec![], vec![], vec![], vec![]);

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

    pub fn create_normals(&mut self)
    {
        let mut mesh_data = self.get_data_mut().get_mut();
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

    fn calc_bbox(&mut self)
    {
        let trans = Isometry3::<f32>::identity();
        let mut data = self.data.get_mut();
        data.b_box = data.mesh.aabb(&trans);
    }

    pub fn intersect_b_box(&self, ray_inverse: &Ray, solid: bool) -> Option<f32>
    {
        let data = self.get_data();
        data.b_box.cast_local_ray(&ray_inverse, std::f32::MAX, solid)
    }

    pub fn intersect(&self, ray: &Ray, ray_inverse: &Ray, trans: &Matrix4<f32>, trans_inverse: &Matrix4<f32>, solid: bool, smooth_shading: bool) -> Option<(f32, Vector3<f32>, u32)>
    {
        let data = self.get_data();

        let res = data.mesh.cast_local_ray_and_get_normal(&ray_inverse, std::f32::MAX, solid);
        if let Some(res) = res
        {
            let mut face_id = 0;
            if let FeatureId::Face(i) = res.feature
            {
                face_id = i;
            }

            let mut normal;

            // use normal based on loaded normal (not on computed normal by parry -- for smooth shading)
            if smooth_shading && data.normals.len() > 0 && data.normals_indices.len() > 0
            {
                let hit = ray.origin + (ray.dir * res.toi);
                normal = self.get_normal(hit, face_id, trans_inverse);
                normal = (trans * normal.to_homogeneous()).xyz().normalize();

                if data.mesh.is_backface(res.feature)
                {
                    normal = -normal;
                }
            }
            else
            {
                normal = (trans * res.normal.to_homogeneous()).xyz().normalize();
            }

            return Some((res.toi, normal, face_id))
        }
        None
    }

    fn apply_transform(&mut self, transform: &Matrix4<f32>)
    {
        let mut data = self.data.get_mut();

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
        data.mesh = TriMesh::new(data.vertices.clone(), data.indices.clone());

        self.calc_bbox();
    }

    pub fn merge(&mut self, mesh_data: &MeshData)
    {
        let data = self.data.get_mut();

        let vertices_offset = data.vertices.len() as u32;
        let normals_offset = data.normals.len() as u32;
        let uv_offset = data.uvs_1.len() as u32;

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

        data.mesh = TriMesh::new(data.vertices.clone(), data.indices.clone());

        self.calc_bbox();
    }

    pub fn merge_by_transformations(&mut self, transformations: &Vec::<Matrix4<f32>>)
    {
        let cloned_vertices;
        let cloned_indices;

        let cloned_uvs_1;
        let cloned_uvs_2;
        let cloned_uvs_3;
        let cloned_uv_indices;

        let cloned_normals;
        let cloned_normals_indices;

        {
            let data = self.get_data();

            cloned_vertices = data.vertices.clone();
            cloned_indices = data.indices.clone();

            cloned_uvs_1 = data.uvs_1.clone();
            cloned_uvs_2 = data.uvs_2.clone();
            cloned_uvs_3 = data.uvs_3.clone();
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
                let normals_offset = data.normals.len() as u32;
                let uv_offset = data.uvs_1.len() as u32;

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

                data.uvs_1.extend(&cloned_uvs_1);
                data.uvs_2.extend(&cloned_uvs_2);
                data.uvs_3.extend(&cloned_uvs_3);

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
            data.mesh = TriMesh::new(data.vertices.clone(), data.indices.clone());
        }

        self.calc_bbox();
    }

    pub fn get_normal(&self, hit: Point3<f32>, face_id: u32, tran_inverse: &Matrix4<f32>) -> Vector3<f32>
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
    component_impl_set_enabled!();

    fn instantiable(&self) -> bool
    {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let data = self.get_data();
        ui.label(format!(" ⚫ vertices: {}", data.vertices.len()));
        ui.label(format!(" ⚫ indices: {}", data.indices.len()));

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

        ui.label(format!(" ⚫ flip_normals: {}", data.flip_normals));
    }
}