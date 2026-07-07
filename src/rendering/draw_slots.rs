use std::collections::HashMap;

use crate::{render_item_impl_default, rendering::wgpu::WGpu, state::helper::render_item::RenderItem};

const MIN_SIZE: usize = 1024; // entries

// one draw slot per (node, mesh) pair - the slot index is the offset into the indirect args buffers
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawSlot
{
    pub node_index: u32,     // index into the bounding boxes / visibility buffers
    pub index_count: u32,
    pub instance_count: u32,
    pub _padding: u32,
}

// matches wgpu::util::DrawIndexedIndirectArgs (base_vertex is always 0 here, so u32 is fine)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedArgs
{
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub base_vertex: u32,
    pub first_instance: u32,
}

pub const DRAW_INDEXED_ARGS_SIZE: u64 = std::mem::size_of::<DrawIndexedArgs>() as u64;

fn full_visible_args(slots: &Vec<DrawSlot>, capacity: usize) -> Vec<DrawIndexedArgs>
{
    // everything visible: safe fallback until the first occlusion check results exist
    let mut args = Vec::with_capacity(capacity);

    for slot in slots
    {
        args.push(DrawIndexedArgs
        {
            index_count: slot.index_count,
            instance_count: slot.instance_count,
            first_index: 0,
            base_vertex: 0,
            first_instance: 0,
        });
    }

    for _ in args.len()..capacity
    {
        args.push(DrawIndexedArgs { index_count: 0, instance_count: 0, first_index: 0, base_vertex: 0, first_instance: 0 });
    }

    args
}

// scene-global draw slot metadata (input for the occlusion check compute pass)
pub struct DrawSlotsBuffer
{
    pub buffer: wgpu::Buffer,
    pub buffer_size: usize, // capacity in slots

    pub slots: Vec<DrawSlot>,               // cpu copy (used to initialize the indirect args buffers)
    pub slot_map: HashMap<u32, (u32, u32)>, // node id -> (first slot index, slot count)
}

impl RenderItem for DrawSlotsBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.buffer.size()
    }
}

impl DrawSlotsBuffer
{
    pub fn new(wgpu: &mut WGpu) -> Self
    {
        let mut buffer = Self
        {
            buffer: crate::rendering::helper::buffer::create_empty_buffer(wgpu),
            buffer_size: 0,
            slots: vec![],
            slot_map: HashMap::new(),
        };

        buffer.update(wgpu, vec![], HashMap::new());

        buffer
    }

    // returns true if the gpu buffer was recreated (bind groups have to be recreated)
    pub fn update(&mut self, wgpu: &mut WGpu, slots: Vec<DrawSlot>, slot_map: HashMap<u32, (u32, u32)>) -> bool
    {
        let new_buffer_size = slots.len().next_power_of_two().max(MIN_SIZE);

        let recreated = new_buffer_size > self.buffer_size;

        if recreated
        {
            self.buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
            {
                label: Some("Draw Slots Buffer"),
                size: (std::mem::size_of::<DrawSlot>() * new_buffer_size) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            self.buffer_size = new_buffer_size;
        }

        // only write the used entries - the shader never reads past num_slots
        // (stale data beyond the used range is fine)
        if !slots.is_empty()
        {
            wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&slots));
        }

        self.slots = slots;
        self.slot_map = slot_map;

        recreated
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }
}

// per camera indirect draw args (written by the occlusion check compute pass):
//  - args_visible: all currently visible objects (pass 1 of the next frame + transparent/no-depth draws)
//  - args_new: objects which became visible this frame (pass 2)
pub struct IndirectArgsBuffers
{
    pub args_visible: wgpu::Buffer,
    pub args_new: wgpu::Buffer,
    pub buffer_size: usize, // capacity in slots
}

impl RenderItem for IndirectArgsBuffers
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.args_visible.size() + self.args_new.size()
    }
}

impl IndirectArgsBuffers
{
    pub fn new(wgpu: &mut WGpu, slots: &Vec<DrawSlot>) -> Self
    {
        let buffer_size = slots.len().next_power_of_two().max(MIN_SIZE);

        let usage = wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let size = (std::mem::size_of::<DrawIndexedArgs>() * buffer_size) as u64;

        let args_visible = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("indirect args buffer (visible)"),
            size,
            usage,
            mapped_at_creation: false,
        });

        let args_new = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("indirect args buffer (newly visible)"),
            size,
            usage,
            mapped_at_creation: false,
        });

        let buffers = Self
        {
            args_visible,
            args_new,
            buffer_size,
        };

        buffers.reset_full_visible(wgpu, slots);

        buffers
    }

    // reset to "everything visible" (safe fallback after slot changes or when occlusion culling is re-enabled)
    pub fn reset_full_visible(&self, wgpu: &mut WGpu, slots: &Vec<DrawSlot>)
    {
        let args = full_visible_args(slots, self.buffer_size);

        wgpu.queue_mut().write_buffer(&self.args_visible, 0, bytemuck::cast_slice(&args));

        // args_new is rewritten by the compute pass before it is consumed - zero counts are enough
        let zero_args = full_visible_args(&vec![], self.buffer_size);
        wgpu.queue_mut().write_buffer(&self.args_new, 0, bytemuck::cast_slice(&zero_args));
    }
}
