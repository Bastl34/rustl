use crate::state::scene::components::mesh::Mesh;

use super::wgpu::WGpu;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex
{
    position: [f32; 3],
    tex_coords: [f32; 2],
    normal: [f32; 3],
}

impl Vertex
{
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3];

    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a>
    {
        use std::mem;

        wgpu::VertexBufferLayout
        {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}


pub struct VertexBuffer
{
    pub name: String,
    vertex_count: u32,
    index_count: u32,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

impl VertexBuffer
{
    pub fn new(wgpu: &mut WGpu, name: &str, mesh: &Mesh) -> VertexBuffer
    {
        let device = wgpu.device();

        /*
        const VERTICES: &[Vertex] = &[
            Vertex { position: [-0.0868241, 0.49240386, 0.0], tex_coords: [0.4131759, 0.00759614], normal: [0.0, 0.0, 0.0]}, // A
            Vertex { position: [-0.49513406, 0.06958647, 0.0], tex_coords: [0.0048659444, 0.43041354], normal: [0.0, 0.0, 0.0] }, // B
            Vertex { position: [-0.21918549, -0.44939706, 0.0], tex_coords: [0.28081453, 0.949397], normal: [0.0, 0.0, 0.0] }, // C
            Vertex { position: [0.35966998, -0.3473291, 0.0], tex_coords: [0.85967, 0.84732914], normal: [0.0, 0.0, 0.0] }, // D
            Vertex { position: [0.44147372, 0.2347359, 0.0], tex_coords: [0.9414737, 0.2652641], normal: [0.0, 0.0, 0.0] }, // E
        ];
        */

        /*
        const INDICES: &[u16] =
        &[
            0, 1, 4,
            1, 2, 4,
            2, 3, 4,
        ];
        */

        let mut face_id: u32 = 0;

        let mut vertices = vec![];

        for i in  0..mesh.vertices.len()
        {
            let v = mesh.mesh.vertices()[i];
            let n = mesh.normals[i];
            let uv = mesh.uvs[i];

            vertices.push(Vertex
            {
                position: [v.x, v.y, v.z],
                tex_coords: [uv.x, uv.y],
                normal: [n.x, n.y, n.z]
            });
        }

        /*
        for face in mesh.mesh.indices()
        {
            let i0 = face[0] as usize;
            let i1 = face[1] as usize;
            let i2 = face[2] as usize;

            let v0 = mesh.mesh.vertices()[i0];
            let v1 = mesh.mesh.vertices()[i1];
            let v2 = mesh.mesh.vertices()[i2];

            let normal_indices = mesh.normals_indices[face_id as usize];
            let uv_indices = mesh.uv_indices [face_id as usize];

            let n0 = mesh.normals[normal_indices[0] as usize];
            let n1 = mesh.normals[normal_indices[1] as usize];
            let n2 = mesh.normals[normal_indices[2] as usize];

            let uv0 = mesh.uvs[uv_indices[0] as usize];
            let uv1 = mesh.uvs[uv_indices[1] as usize];
            let uv2 = mesh.uvs[uv_indices[2] as usize];

            let verts = [v0, v1, v2];
            let normals = [n0, n1, n2];
            let uvs = [uv0, uv1, uv2];

            for i in 0..=2
            {
                vertices.push(Vertex
                {
                    position: [verts[i].x, verts[i].y, verts[i].z],
                    tex_coords: [uvs[i].x, uvs[i].y],
                    normal: [normals[i].x, normals[i].y, normals[i].z]
                });
            }

            face_id += 1;
        }
        */

        dbg!(vertices.len());
        dbg!(mesh.mesh.vertices().len());

        //let indices = mesh.get_index_array();
        //let indices = mesh.normals_indices.clone();
        let indices = mesh.indices.clone();

        let vertex_buffer_name = format!("{} Vertex Buffer", name);
        let vertex_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(vertex_buffer_name.as_str()),
                contents: bytemuck::cast_slice(vertices.as_slice()),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer_name = format!("{} Index Buffer", name);
        let index_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(index_buffer_name.as_str()),
                contents: bytemuck::cast_slice(indices.as_slice()),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        Self
        {
            name: name.to_string(),
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,

            vertex_buffer: vertex_buffer,
            index_buffer: index_buffer,
        }
    }

    pub fn get_vertex_buffer(&self) -> &wgpu::Buffer
    {
        &self.vertex_buffer
    }

    pub fn get_index_buffer(&self) -> &wgpu::Buffer
    {
        &self.index_buffer
    }

    pub fn get_vertex_count(&self) -> u32
    {
        self.vertex_count
    }

    pub fn get_index_count(&self) -> u32
    {
        self.index_count
    }
}
