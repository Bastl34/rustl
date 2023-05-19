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

        let mut vertices = vec![];

        for i in  0..mesh.vertices.len()
        {
            let v = mesh.vertices[i];
            let n = mesh.normals[i];
            let uv = mesh.uvs[i];

            vertices.push(Vertex
            {
                position: [v.x, v.y, v.z],
                tex_coords: [uv.x, uv.y],
                normal: [n.x, n.y, n.z]
            });
        }

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
                contents: bytemuck::cast_slice(mesh.indices.as_slice()),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        Self
        {
            name: name.to_string(),
            vertex_count: vertices.len() as u32,
            index_count: (mesh.indices.len() as u32) * 3,

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
