
// Due to uniforms requiring 16 byte (4 float) spacing, its needed to use pading
// position: [f32; 3]
// _padding: [f32; 1]
// --> 16
// https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-blinn-phong-model
// https://www.w3.org/TR/WGSL/#alignment-and-size

use std::{mem, cell::RefCell};

use nalgebra::Point3;

use crate::{console_warning, helper::change_tracker::ChangeTracker, render_item_impl_default, state::{helper::render_item::RenderItem, scene::light::{Light, LightItem, LightType}}};

use super::{shadow, wgpu::WGpu, helper::buffer::create_empty_buffer};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform
{
    pub position: [f32; 4],
    pub dir: [f32; 4],
    pub color: [f32; 4],
    pub ground_color: [f32; 4],
    pub intensity: f32,
    pub range: f32,
    pub light_type: u32,        //0 = disabled, ...
    pub max_angle: f32,
    pub distance_based_intensity: u32,

    // shadow mapping: first layer in the shadow atlas (-1 = light casts no shadow)
    pub shadow_index: i32,
    pub shadow_views: u32,
    pub shadow_bias: f32,
    pub shadow_strength: f32,

    // uniform array stride must be a multiple of 16 bytes
    pub _padding: [f32; 3],
}

impl LightUniform
{
    pub fn new(light: &Light, shadow_index: i32, shadow_views: u32) -> Self
    {
        let mut l_type;
        match light.light_type
        {
            LightType::Directional => l_type = 1,
            LightType::Point => l_type = 2,
            LightType::Spot => l_type = 3,
            LightType::Hemispheric => l_type = 4,

            // a sun is a directional light for the shader - only color/intensity differ (see below)
            LightType::Sun => l_type = 1,
        };

        if !light.enabled
        {
            l_type = 0;
        }

        let dist_based_intensity; if light.distance_based_intensity { dist_based_intensity = 1; } else { dist_based_intensity = 0; }

        let dir_normalized = light.dir_normalized();

        let position: Point3<f32> = light.pos;
        let ground_color = light.ground_color;

        // sun: modulate the user color (tint) with the elevation based sun color
        // and fade the intensity out below the horizon
        let mut color = light.color;
        let mut intensity = light.intensity;
        if light.light_type == LightType::Sun
        {
            color = color.component_mul(&light.sun_color());
            intensity *= light.sun_intensity_factor();
        }

        Self
        {
            position: [position.x, position.y, position.z, 1.0],
            dir: [dir_normalized.x, dir_normalized.y, dir_normalized.z, 1.0],
            color: [color.x, color.y, color.z, 1.0],
            ground_color: [ground_color.x, ground_color.y, ground_color.z, 1.0],
            intensity,
            range: light.range,
            light_type: l_type,
            max_angle: light.max_angle,
            distance_based_intensity: dist_based_intensity,

            shadow_index,
            shadow_views,
            shadow_bias: light.shadow_bias,
            shadow_strength: light.shadow_strength.clamp(0.0, 1.0),

            _padding: [0.0; 3],
        }
    }
}

pub struct LightBuffer
{
    pub name: String,

    max_lights: usize,

    lights_amount: wgpu::Buffer,
    lights_buffer: wgpu::Buffer,
}

impl RenderItem for LightBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.lights_amount.size() + self.lights_buffer.size()
    }
}

impl LightBuffer
{
    pub fn new(wgpu: &mut WGpu, name: String, lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: u32, shadows_enabled: bool) -> LightBuffer
    {
        let mut buffer = LightBuffer
        {
            name: name,
            max_lights: max_lights as usize,
            lights_amount: create_empty_buffer(wgpu),
            lights_buffer: create_empty_buffer(wgpu),
        };


        buffer.create_buffer(wgpu);
        buffer.to_buffer(wgpu, lights, shadows_enabled);

        buffer
    }

    fn uniform_size(max_lights: usize) -> wgpu::BufferAddress
    {
        (max_lights * mem::size_of::<LightUniform>()) as wgpu::BufferAddress
    }

    pub fn create_buffer(&mut self, wgpu: &mut WGpu)
    {
        self.lights_amount = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("lights amount buffer"),
            size: mem::size_of::<u32>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.lights_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some(&self.name),
            size: Self::uniform_size(self.max_lights),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, lights: &Vec<RefCell<ChangeTracker<LightItem>>>, shadows_enabled: bool)
    {
        let amount = lights.len().min(self.max_lights) as u32;

        wgpu.queue_mut().write_buffer
        (
            &self.lights_amount,
            0,
            bytemuck::bytes_of(&amount),
        );

        // shadow atlas layer assignment (must match rendering::shadow::compute_shadow_views)
        // shadows disabled -> shadow_index = -1 for all lights (shader skips the shadow sampling)
        let shadow_assignments = if shadows_enabled
        {
            shadow::assign_shadow_views(lights, self.max_lights)
        }
        else
        {
            vec![(-1, 0); lights.len()]
        };

        for (i, light) in lights.iter().enumerate()
        {
            if i + 1 > self.max_lights
            {
                console_warning!("only {} lights are supported", self.max_lights);
                break;
            }

            let light = light.borrow();
            let light = light.get_ref();

            let (shadow_index, shadow_views) = shadow_assignments[i];
            let data = LightUniform::new(light, shadow_index, shadow_views);

            wgpu.queue_mut().write_buffer
            (
                &self.lights_buffer,
                (i * mem::size_of::<LightUniform>()) as wgpu::BufferAddress,
                bytemuck::bytes_of(&data),
            );
        }
    }

    pub fn get_amount_buffer(&self) -> &wgpu::Buffer
    {
        &self.lights_amount
    }

    pub fn get_lights_buffer(&self) -> &wgpu::Buffer
    {
        &self.lights_buffer
    }
}