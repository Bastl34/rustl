use crate::{state::{scene::components::mesh::{Mesh, MeshData}, helper::render_item::RenderItem}, render_item_impl_default};

use super::wgpu::WGpu;
use nalgebra::Point2;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex
{
    position: [f32; 3],
    tex_coords: [f32; 2],
    normal: [f32; 3]
}

impl Vertex
{
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x3, 4 => Float32x3];

    pub fn desc() -> wgpu::VertexBufferLayout<'static>
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

impl RenderItem for VertexBuffer
{
    render_item_impl_default!();
}

impl VertexBuffer
{
    pub fn new(wgpu: &mut WGpu, name: &str, mesh_data: &MeshData) -> VertexBuffer
    {
        let device = wgpu.device();

        let mut vertices = vec![];

        for i in  0..mesh_data.vertices.len()
        {
            let v = mesh_data.vertices[i];
            let n = mesh_data.normals[i];

            // no uvs found -> use empty uv
            let uv;
            if mesh_data.uvs_1.len() == 0
            {
                uv = Point2::<f32>::new(0.0, 0.0);
            }
            else
            {
                uv = mesh_data.uvs_1[i];
            }

            vertices.push(Vertex
            {
                position: [v.x, v.y, v.z],
                tex_coords: [uv.x, 1.0 - uv.y], // flip y because in wgpu y-axis is pointing up (not down as in images)
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
                contents: bytemuck::cast_slice(mesh_data.indices.as_slice()),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        Self
        {
            name: name.to_string(),
            vertex_count: vertices.len() as u32,
            index_count: (mesh_data.indices.len() as u32) * 3,

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
