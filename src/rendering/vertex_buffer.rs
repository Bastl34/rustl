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
    pub fn new(wgpu: &mut WGpu, name: &str) -> VertexBuffer
    {
        let device = wgpu.device();

        const VERTICES: &[Vertex] = &[
            Vertex { position: [-0.0868241, 0.49240386, 0.0], tex_coords: [0.4131759, 0.00759614], normal: [0.0, 0.0, 0.0]}, // A
            Vertex { position: [-0.49513406, 0.06958647, 0.0], tex_coords: [0.0048659444, 0.43041354], normal: [0.0, 0.0, 0.0] }, // B
            Vertex { position: [-0.21918549, -0.44939706, 0.0], tex_coords: [0.28081453, 0.949397], normal: [0.0, 0.0, 0.0] }, // C
            Vertex { position: [0.35966998, -0.3473291, 0.0], tex_coords: [0.85967, 0.84732914], normal: [0.0, 0.0, 0.0] }, // D
            Vertex { position: [0.44147372, 0.2347359, 0.0], tex_coords: [0.9414737, 0.2652641], normal: [0.0, 0.0, 0.0] }, // E
        ];

        const INDICES: &[u16] =
        &[
            0, 1, 4,
            1, 2, 4,
            2, 3, 4,
        ];

        let vertex_buffer_name = format!("{} Vertex Buffer", name);
        let vertex_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(vertex_buffer_name.as_str()),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            }
        );

        let index_buffer_name = format!("{} Index Buffer", name);
        let index_buffer = device.create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(index_buffer_name.as_str()),
                contents: bytemuck::cast_slice(INDICES),
                usage: wgpu::BufferUsages::INDEX,
            }
        );

        Self
        {
            name: name.to_string(),
            vertex_count: VERTICES.len() as u32,
            index_count: INDICES.len() as u32,

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
