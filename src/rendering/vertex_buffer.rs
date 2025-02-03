use crate::{helper::stopwatch::StopWatch, render_item_impl_default, state::{helper::render_item::RenderItem, scene::components::mesh::MeshData}};

use super::wgpu::WGpu;
use nalgebra::{Point2, Vector3};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex
{
    position: [f32; 3],

    tex_coords_0_1: [f32; 4],
    tex_coords_2_3: [f32; 4],

    normal: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],

    joints: [u32; 4],
    weights: [f32; 4],
}

pub const VERTEX_ATTRIBUTES_AMOUNT: usize = 8;

impl Vertex
{
    const ATTRIBS: [wgpu::VertexAttribute; VERTEX_ATTRIBUTES_AMOUNT] = wgpu::vertex_attr_array!
    [
        0 => Float32x3,

        1 => Float32x4,
        2 => Float32x4,

        3 => Float32x3,
        4 => Float32x3,
        5 => Float32x3,

        6 => Uint32x4,
        7 => Float32x4
    ];

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
            let uv_0;
            if mesh_data.uvs_0.len() == 0
            {
                uv_0 = Point2::<f32>::new(0.0, 0.0);
            }
            else
            {
                uv_0 = mesh_data.uvs_0[i];
            }

            let uv_1;
            if mesh_data.uvs_1.len() == 0
            {
                uv_1 = Point2::<f32>::new(0.0, 0.0);
            }
            else
            {
                uv_1 = mesh_data.uvs_1[i];
            }

            let uv_2;
            if mesh_data.uvs_2.len() == 0
            {
                uv_2 = Point2::<f32>::new(0.0, 0.0);
            }
            else
            {
                uv_2 = mesh_data.uvs_2[i];
            }

            let uv_3;
            if mesh_data.uvs_3.len() == 0
            {
                uv_3 = Point2::<f32>::new(0.0, 0.0);
            }
            else
            {
                uv_3 = mesh_data.uvs_3[i];
            }

            let mut tangent = n.cross(&Vector3::<f32>::new(0.0, 1.0, 0.0));
            if tangent.magnitude()  <= 0.0001
            {
                tangent = n.cross(&Vector3::<f32>::new(0.0, 0.0, 1.0));
            }

            tangent = tangent.normalize();
            let bitangent = n.cross(&tangent).normalize();

            let mut joints = [0, 0, 0, 0];
            let mut weights = [0.0, 0.0, 0.0, 0.0];

            let joints_data = mesh_data.joints.get(i);
            if let Some(joints_data) = joints_data
            {
                joints[0] = joints_data[0];
                joints[1] = joints_data[1];
                joints[2] = joints_data[2];
                joints[3] = joints_data[3];
            }

            let weights_data = mesh_data.weights.get(i);
            if let Some(weights_data) = weights_data
            {
                weights[0] = weights_data[0];
                weights[1] = weights_data[1];
                weights[2] = weights_data[2];
                weights[3] = weights_data[3];
            }

            vertices.push(Vertex
            {
                position: [v.x, v.y, v.z],

                // flip y because in wgpu y-axis is pointing up (not down as in images)
                tex_coords_0_1:
                [
                    uv_0.x, 1.0 - uv_0.y,
                    uv_1.x, 1.0 - uv_1.y
                ],
                tex_coords_2_3:
                [
                    uv_2.x, 1.0 - uv_2.y,
                    uv_3.x, 1.0 - uv_3.y
                ],

                normal: [n.x, n.y, n.z],
                tangent: [tangent.x, tangent.y, tangent.z],
                bitangent: [bitangent.x, bitangent.y, bitangent.z],
                joints,
                weights
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
