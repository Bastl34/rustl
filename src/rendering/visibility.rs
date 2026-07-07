
use std::sync::{Arc, Mutex, atomic::{AtomicU8, AtomicUsize, Ordering}};

use wgpu::util::DeviceExt;

use crate::{console_warning, render_item_impl_default, rendering::wgpu::WGpu, state::{helper::render_item::RenderItem}};

const MIN_SIZE: usize = 1024; // entries (buffers grow on demand)

// async stats readback states
const READBACK_IDLE: u8 = 0;      // no copy in flight -> a new copy can be recorded
const READBACK_COPIED: u8 = 1;    // copy recorded/submitted -> the buffer can be mapped next frame
const READBACK_MAPPING: u8 = 2;   // map requested -> waiting for the callback

// map_async callback results
const MAP_PENDING: u8 = 0;
const MAP_OK: u8 = 1;
const MAP_ERROR: u8 = 2;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct Visibility
{
    pub object_id: u32,
    pub visible: u32,
}

pub struct VisibilityBuffer
{
    pub storage_buffer: wgpu::Buffer, // visibility of the current frame (written by the occlusion check)
    pub prev_buffer: wgpu::Buffer,    // visibility of the previous frame (copied from storage_buffer each frame)
    pub readback_buffer: wgpu::Buffer,
    pub buffer_size: usize,

    // async stats readback (interior mutability - the cameras are not mutable during rendering)
    readback_state: AtomicU8,
    readback_count: AtomicUsize, // number of valid entries at the time the copy was recorded
    map_result: Arc<AtomicU8>,
    results: Mutex<Vec<Visibility>>, // latest read back results (a few frames behind - stats/debug only)
}

impl RenderItem for VisibilityBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.storage_buffer.size() + self.prev_buffer.size() + self.readback_buffer.size()
    }
}

impl VisibilityBuffer
{
    pub fn new(wgpu: &mut WGpu, num_objects: usize) -> Self
    {
        let buffer_size = num_objects.next_power_of_two().max(MIN_SIZE);

        // start with everything visible: pass 1 renders all objects until the first occlusion results exist
        let init_data = vec![Visibility { object_id: 0, visible: 1 }; buffer_size];

        let visibility_buffer = wgpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor
        {
            label: Some("visibility buffer"),
            contents: bytemuck::cast_slice(&init_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });

        let prev_buffer = wgpu.device().create_buffer_init(&wgpu::util::BufferInitDescriptor
        {
            label: Some("visibility prev buffer"),
            contents: bytemuck::cast_slice(&init_data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        let readback_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("visibility readback buffer"),
            size: (std::mem::size_of::<Visibility>() * buffer_size) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self
        {
            storage_buffer: visibility_buffer,
            prev_buffer,
            readback_buffer,
            buffer_size,

            readback_state: AtomicU8::new(READBACK_IDLE),
            readback_count: AtomicUsize::new(0),
            map_result: Arc::new(AtomicU8::new(MAP_PENDING)),
            results: Mutex::new(vec![]),
        }
    }

    pub fn get_storage_buffer(&self) -> &wgpu::Buffer
    {
        &self.storage_buffer
    }

    pub fn get_prev_buffer(&self) -> &wgpu::Buffer
    {
        &self.prev_buffer
    }

    // reset the visibility to "everything visible" (used when the draw slots change or occlusion culling is re-enabled)
    pub fn reset_all_visible(&self, wgpu: &mut WGpu)
    {
        let init_data = vec![Visibility { object_id: 0, visible: 1 }; self.buffer_size];

        wgpu.queue_mut().write_buffer(&self.storage_buffer, 0, bytemuck::cast_slice(&init_data));
        wgpu.queue_mut().write_buffer(&self.prev_buffer, 0, bytemuck::cast_slice(&init_data));
    }

    // current frame visibility -> previous frame visibility (recorded after the occlusion check pass)
    pub fn copy_current_to_prev(&self, encoder: &mut wgpu::CommandEncoder, num_objects: usize)
    {
        let count = num_objects.min(self.buffer_size);
        if count == 0 { return; }

        encoder.copy_buffer_to_buffer(&self.storage_buffer, 0, &self.prev_buffer, 0, (std::mem::size_of::<Visibility>() * count) as u64);
    }

    // record a copy for the async stats readback (only when no readback is in flight)
    pub fn record_readback_copy(&self, encoder: &mut wgpu::CommandEncoder, num_objects: usize)
    {
        if self.readback_state.load(Ordering::SeqCst) != READBACK_IDLE
        {
            return;
        }

        let count = num_objects.min(self.buffer_size);
        if count == 0 { return; }

        encoder.copy_buffer_to_buffer(&self.storage_buffer, 0, &self.readback_buffer, 0, (std::mem::size_of::<Visibility>() * count) as u64);

        // the object count can change while the readback is in flight -> remember the snapshot count
        self.readback_count.store(count, Ordering::SeqCst);
        self.readback_state.store(READBACK_COPIED, Ordering::SeqCst);
    }

    // non-blocking: advance the readback state machine and update the cached results if new data arrived
    pub fn update_readback(&self, wgpu: &mut WGpu)
    {
        match self.readback_state.load(Ordering::SeqCst)
        {
            READBACK_COPIED =>
            {
                // the copy was submitted at the end of the last frame -> the buffer can be mapped now
                let count = self.readback_count.load(Ordering::SeqCst);
                let map_result = self.map_result.clone();

                self.map_result.store(MAP_PENDING, Ordering::SeqCst);
                self.readback_buffer.slice(..(std::mem::size_of::<Visibility>() * count) as u64).map_async(wgpu::MapMode::Read, move |result|
                {
                    map_result.store(if result.is_ok() { MAP_OK } else { MAP_ERROR }, Ordering::SeqCst);
                });

                self.readback_state.store(READBACK_MAPPING, Ordering::SeqCst);

                let _ = wgpu.device().poll(wgpu::PollType::Poll);
            },
            READBACK_MAPPING =>
            {
                let _ = wgpu.device().poll(wgpu::PollType::Poll);
            },
            _ => {}
        }

        if self.readback_state.load(Ordering::SeqCst) != READBACK_MAPPING
        {
            return;
        }

        match self.map_result.load(Ordering::SeqCst)
        {
            MAP_OK =>
            {
                let count = self.readback_count.load(Ordering::SeqCst);

                {
                    let data = self.readback_buffer.slice(..(std::mem::size_of::<Visibility>() * count) as u64).get_mapped_range();

                    let count_in_bytes = (count * std::mem::size_of::<Visibility>()).min(data.len());
                    let result = bytemuck::cast_slice::<u8, Visibility>(&data[..count_in_bytes]).to_vec();

                    *self.results.lock().unwrap() = result;
                }

                self.readback_buffer.unmap();
                self.map_result.store(MAP_PENDING, Ordering::SeqCst);
                self.readback_state.store(READBACK_IDLE, Ordering::SeqCst);
            },
            MAP_ERROR =>
            {
                // a failed map must not wedge the state machine -> reset and try again with a fresh copy
                console_warning!("visibility readback map failed -> retrying");

                self.map_result.store(MAP_PENDING, Ordering::SeqCst);
                self.readback_state.store(READBACK_IDLE, Ordering::SeqCst);
            },
            _ => {}
        }
    }

    // latest read back visibility results (a few frames behind - stats/debug only)
    pub fn latest_results(&self) -> Vec<Visibility>
    {
        self.results.lock().unwrap().clone()
    }
}
