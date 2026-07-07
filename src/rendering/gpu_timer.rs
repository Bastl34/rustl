
use std::collections::VecDeque;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::rendering::wgpu::WGpu;

const ROLLING_WINDOW: usize = 30; // frames used for the displayed average
const MAX_QUERIES: u32 = 64; // 2 per timed segment (shadow block + depth/color/hzb per camera)

pub const GPU_TIMER_PASSES: usize = 5;

#[derive(Copy, Clone)]
pub enum GpuTimerPass
{
    Shadow = 0,
    Depth,
    Color,
    Hzb,
    Egui,
}

// averaged gpu time per pass block in ms (None = pass did not run / no results yet)
#[derive(Copy, Clone, Default)]
pub struct GpuPassTimes
{
    pub shadow: Option<f32>,
    pub depth: Option<f32>,
    pub color: Option<f32>,
    pub hzb: Option<f32>,
    pub egui: Option<f32>,
}

// one timed block (a pair of timestamp query slots)
// the timestamps are written by the passes itself (timestamp_writes at the pass boundaries):
// the first pass of the block writes begin_index and the last pass of the block writes end_index
#[derive(Copy, Clone)]
pub struct GpuTimerSegment<'a>
{
    query_set: &'a wgpu::QuerySet,
    begin_index: u32,
    end_index: u32,
}

impl<'a> GpuTimerSegment<'a>
{
    // for a block which consists of a single render pass
    pub fn full_render_writes(&self) -> wgpu::RenderPassTimestampWrites<'a>
    {
        wgpu::RenderPassTimestampWrites
        {
            query_set: self.query_set,
            beginning_of_pass_write_index: Some(self.begin_index),
            end_of_pass_write_index: Some(self.end_index),
        }
    }

    pub fn begin_render_writes(&self) -> wgpu::RenderPassTimestampWrites<'a>
    {
        wgpu::RenderPassTimestampWrites
        {
            query_set: self.query_set,
            beginning_of_pass_write_index: Some(self.begin_index),
            end_of_pass_write_index: None,
        }
    }

    pub fn end_render_writes(&self) -> wgpu::RenderPassTimestampWrites<'a>
    {
        wgpu::RenderPassTimestampWrites
        {
            query_set: self.query_set,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(self.end_index),
        }
    }

    pub fn end_compute_writes(&self) -> wgpu::ComputePassTimestampWrites<'a>
    {
        wgpu::ComputePassTimestampWrites
        {
            query_set: self.query_set,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(self.end_index),
        }
    }

    // timestamp writes for one pass of a block which consists of multiple render passes:
    // the first pass writes the begin timestamp and the last pass writes the end timestamp
    // (if the block has just one pass, it writes both - passes in between write nothing)
    pub fn render_writes_for_pass(&self, pass_index: usize, pass_amount: usize) -> Option<wgpu::RenderPassTimestampWrites<'a>>
    {
        let first_pass = pass_index == 0;
        let last_pass = pass_index + 1 == pass_amount;

        if first_pass && last_pass
        {
            Some(self.full_render_writes())
        }
        else if first_pass
        {
            Some(self.begin_render_writes())
        }
        else if last_pass
        {
            Some(self.end_render_writes())
        }
        else
        {
            None
        }
    }
}

pub struct GpuTimer
{
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,

    // queries/segments of the frame which is currently recorded
    used_queries: u32,
    segments: Vec<(usize, u32, u32)>, // (pass, begin query index, end query index)

    // queries/segments of the frame which was resolved into the readback buffer
    pending_queries: u32,
    pending_segments: Vec<(usize, u32, u32)>,
    readback_pending: bool,
    map_requested: bool,
    map_ready: Arc<AtomicBool>,

    times: [VecDeque<f32>; GPU_TIMER_PASSES], // rolling window per pass (ms)
    averages: [Option<f32>; GPU_TIMER_PASSES],
}

impl GpuTimer
{
    pub fn new(device: &wgpu::Device) -> Option<GpuTimer>
    {
        // the timestamps are written via timestamp_writes at the pass boundaries
        // (encoder.write_timestamp is not used on purpose: on metal it needs deferred sampling
        // via dummy encoders which corrupts the output - and browsers do not expose it anyway)
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }

        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor
        {
            label: Some("gpu timer query set"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_QUERIES,
        });

        let buffer_size = (MAX_QUERIES as usize * std::mem::size_of::<u64>()) as u64;

        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("gpu timer resolve buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("gpu timer readback buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Some(Self
        {
            query_set,
            resolve_buffer,
            readback_buffer,

            used_queries: 0,
            segments: vec![],

            pending_queries: 0,
            pending_segments: vec![],
            readback_pending: false,
            map_requested: false,
            map_ready: Arc::new(AtomicBool::new(false)),

            times: std::array::from_fn(|_| VecDeque::new()),
            averages: [None; GPU_TIMER_PASSES],
        })
    }

    // allocate a timestamp pair for one pass block
    // the caller has to attach the returned writes to the passes of the block
    // (both indices have to be written - otherwise the resolve would read unwritten queries)
    pub fn begin_segment(&mut self, pass: GpuTimerPass) -> Option<GpuTimerSegment<'_>>
    {
        if self.used_queries + 2 > MAX_QUERIES
        {
            return None;
        }

        let begin_index = self.used_queries;
        let end_index = begin_index + 1;

        self.segments.push((pass as usize, begin_index, end_index));
        self.used_queries += 2;

        Some(GpuTimerSegment
        {
            query_set: &self.query_set,
            begin_index,
            end_index,
        })
    }

    // resolve the recorded timestamps into the readback buffer (read in one of the next frames)
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder)
    {
        // do not touch the readback buffer while the last resolved results are not read yet
        if self.readback_pending || self.used_queries == 0
        {
            return;
        }

        encoder.resolve_query_set(&self.query_set, 0..self.used_queries, &self.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(&self.resolve_buffer, 0, &self.readback_buffer, 0, self.used_queries as u64 * std::mem::size_of::<u64>() as u64);

        self.pending_queries = self.used_queries;
        std::mem::swap(&mut self.pending_segments, &mut self.segments);
        self.readback_pending = true;
    }

    // read back the timestamps of the previous frame (non-blocking - same idea as the visibility readback)
    // and start the recording of a new frame
    pub fn read_back_results(&mut self, wgpu: &mut WGpu)
    {
        self.used_queries = 0;
        self.segments.clear();

        if !self.readback_pending
        {
            return;
        }

        if !self.map_requested
        {
            let map_ready = self.map_ready.clone();

            self.readback_buffer.slice(..).map_async(wgpu::MapMode::Read, move |result|
            {
                if result.is_ok()
                {
                    map_ready.store(true, Ordering::SeqCst);
                }
            });

            self.map_requested = true;
        }

        let _ = wgpu.device().poll(wgpu::PollType::Poll);

        if !self.map_ready.load(Ordering::SeqCst)
        {
            // results are not ready yet -> check again next frame
            return;
        }

        // sum up the time per pass (a pass can have multiple segments - one per camera)
        let timestamp_period = wgpu.queue_mut().get_timestamp_period(); // nanoseconds per timestamp tick
        let mut pass_times: [Option<f64>; GPU_TIMER_PASSES] = [None; GPU_TIMER_PASSES];
        {
            let data = self.readback_buffer.slice(..).get_mapped_range();
            let timestamps = bytemuck::cast_slice::<u8, u64>(&data[..self.pending_queries as usize * std::mem::size_of::<u64>()]);

            for (pass, begin, end) in &self.pending_segments
            {
                let ticks = timestamps[*end as usize].saturating_sub(timestamps[*begin as usize]);
                let ms = ticks as f64 * timestamp_period as f64 / 1_000_000.0;

                pass_times[*pass] = Some(pass_times[*pass].unwrap_or(0.0) + ms);
            }
        }

        self.readback_buffer.unmap();
        self.map_ready.store(false, Ordering::SeqCst);
        self.map_requested = false;
        self.readback_pending = false;

        // rolling average over the last ROLLING_WINDOW frames
        for (pass, time) in pass_times.iter().enumerate()
        {
            if let Some(time) = time
            {
                let times = &mut self.times[pass];

                times.push_back(*time as f32);

                while times.len() > ROLLING_WINDOW
                {
                    times.pop_front();
                }

                self.averages[pass] = Some(times.iter().sum::<f32>() / times.len() as f32);
            }
            else
            {
                // the pass did not run -> do not show outdated values
                self.times[pass].clear();
                self.averages[pass] = None;
            }
        }
    }

    pub fn pass_times(&self) -> GpuPassTimes
    {
        GpuPassTimes
        {
            shadow: self.averages[GpuTimerPass::Shadow as usize],
            depth: self.averages[GpuTimerPass::Depth as usize],
            color: self.averages[GpuTimerPass::Color as usize],
            hzb: self.averages[GpuTimerPass::Hzb as usize],
            egui: self.averages[GpuTimerPass::Egui as usize],
        }
    }
}
