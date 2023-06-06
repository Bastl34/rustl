use image::{DynamicImage, ImageBuffer, Rgba};
use wgpu::{Device, Queue, Surface, SurfaceCapabilities, SurfaceConfiguration, CommandEncoder, TextureView, SurfaceTexture, Buffer, Texture};

use crate::helper::image::brga_to_rgba;

use super::helper::buffer::{BufferDimensions, remove_padding};

pub struct WGpu
{
    device: Device,
    queue: Queue,
    surface: Surface,

    surface_config: SurfaceConfiguration,
    pub surface_caps: SurfaceCapabilities,
}

impl WGpu
{
    pub async fn new(window: &winit::window::Window) -> Self
    {
        let dimensions = window.inner_size();

        let instance_desc = wgpu::InstanceDescriptor::default();
        //instance_desc.backends = wgpu::Backends::DX12;

        let instance = wgpu::Instance::new(instance_desc);
        let surface = unsafe { instance.create_surface(window) }.unwrap();

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions
        {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .unwrap();

        println!(" ********** info **********");
        dbg!(adapter.get_info());

        println!(" ********** features possible **********");
        dbg!(adapter.features());

        println!(" ********** limits possible **********");
        dbg!(adapter.limits());

        let (device, queue) = adapter.request_device
        (
            &wgpu::DeviceDescriptor
            {
                label: None,
                features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if building for the web: disable some
                limits: if cfg!(target_arch = "wasm32")
                {
                    wgpu::Limits::downlevel_webgl2_defaults()
                }
                else
                {
                    wgpu::Limits::default()
                },
            },
            None,
        )
        .await
        .unwrap();

        println!(" ********** features used **********");
        dbg!(device.features());

        println!(" ********** limits used **********");
        dbg!(device.limits());

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_config = wgpu::SurfaceConfiguration
        {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            width: dimensions.width,
            height: dimensions.height,
            //present_mode: surface_caps.present_modes[0], //wgpu::PresentMode::Fifo
            //present_mode: wgpu::PresentMode::AutoNoVsync,
            present_mode: wgpu::PresentMode::Fifo,
            //alpha_mode: surface_caps.alpha_modes[0], //wgpu::CompositeAlphaMode::Auto
            alpha_mode: surface_caps.alpha_modes[0], //wgpu::CompositeAlphaMode::Auto
            format: surface_caps.formats[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        Self
        {
            device,
            surface,
            queue,
            surface_caps,
            surface_config
        }
    }

    pub fn device(&self) -> &Device
    {
        &self.device
    }

    pub fn queue_mut(&self) -> &Queue
    {
        &self.queue
    }

    pub fn surface_config(&self) -> &SurfaceConfiguration
    {
        &self.surface_config
    }

    pub fn resize(&mut self, dimensions: winit::dpi::PhysicalSize<u32>)
    {
        self.surface_config.width = dimensions.width;
        self.surface_config.height = dimensions.height;

        self.surface.configure(&self.device, &self.surface_config);
    }

    pub fn start_render(&mut self) -> (SurfaceTexture, TextureView, CommandEncoder)
    {
        let output = self.surface.get_current_texture().unwrap();

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        (output, view, encoder)
    }

    pub fn end_render(&mut self, output: SurfaceTexture, encoder: CommandEncoder)
    {
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn start_screenshot_render(&mut self) -> (BufferDimensions, Buffer, Texture, TextureView, CommandEncoder)
    {
        let buffer_dimensions = BufferDimensions::new(self.surface_config.width as usize, self.surface_config.height as usize);

        // The output buffer lets us retrieve the data as an array
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor
        {
            label: None,
            size: (buffer_dimensions.padded_bytes_per_row * buffer_dimensions.height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let texture_extent = wgpu::Extent3d
        {
            width: buffer_dimensions.width as u32,
            height: buffer_dimensions.height as u32,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor
        {
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            label: None,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        (buffer_dimensions, output_buffer, texture, view, encoder)
    }

    pub fn end_screenshot_render(&mut self, buffer_dimensions: BufferDimensions, output_buffer: Buffer, texture: Texture, mut encoder: CommandEncoder) -> DynamicImage
    {
        let texture_extent = wgpu::Extent3d
        {
            width: buffer_dimensions.width as u32,
            height: buffer_dimensions.height as u32,
            depth_or_array_layers: 1,
        };

        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer
        (
            texture.as_image_copy(),
            wgpu::ImageCopyBuffer
            {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout
                {
                    offset: 0,
                    bytes_per_row: Some(buffer_dimensions.padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            texture_extent,
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // read buffer
        let slice: wgpu::BufferSlice = output_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| ());
        self.device.poll(wgpu::Maintain::Wait);

        // remove padding
        let padded_data = slice.get_mapped_range();
        let data = remove_padding(&padded_data, &buffer_dimensions);
        drop(padded_data);

        output_buffer.unmap();

        let img = DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::from_raw(buffer_dimensions.width as u32, buffer_dimensions.height as u32, data).unwrap());
        brga_to_rgba(img)
    }

}
