use std::mem::size_of;

use wgpu::BufferView;

use crate::rendering::wgpu::WGpu;

// wgpu requires texture -> buffer copies to be aligned using wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
// Because of this its needed to save both the padded_bytes_per_row as well as the unpadded_bytes_per_row

pub struct BufferDimensions
{
    pub width: usize,
    pub height: usize,
    pub unpadded_bytes_per_row: usize,
    pub padded_bytes_per_row: usize,
}

impl BufferDimensions
{
    pub fn new(width: usize, height: usize) -> Self
    {
        let bytes_per_pixel = size_of::<u32>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;

        Self
        {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
}

pub fn remove_padding_by_dimensions(padded_data: &BufferView, buffer_dimensions: &BufferDimensions) -> Vec<u8>
{
    padded_data
        .chunks(buffer_dimensions.padded_bytes_per_row as _)
        .map(|chunk| { &chunk[..buffer_dimensions.unpadded_bytes_per_row as _]})
        .flatten()
        .map(|x| { *x })
        .collect::<Vec<_>>()
}

pub fn create_empty_buffer(wgpu: &mut WGpu) -> wgpu::Buffer
{
    wgpu.device().create_buffer(&wgpu::BufferDescriptor
    {
        label: Some("Empty Buffer"),
        size: 0,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}