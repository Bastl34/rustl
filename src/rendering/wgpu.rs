use image::{DynamicImage, ImageBuffer, Rgba};
use wgpu::{Device, Queue, Surface, SurfaceCapabilities, SurfaceConfiguration, CommandEncoder, TextureView, SurfaceTexture, Buffer, Texture};

use crate::{helper::{image::brga_to_rgba, platform::is_windows, concurrency::thread::sleep_millis}, state::state::State};

use super::helper::buffer::{BufferDimensions, remove_padding};

pub struct WGpu
{
    device: Device,
    queue: Queue,
    surface: Surface,

    msaa_samples: u32,
    msaa_texture: Option<wgpu::Texture>,

    surface_config: SurfaceConfiguration,
    pub surface_caps: SurfaceCapabilities,
}

impl WGpu
{
    pub async fn new(window: &winit::window::Window, state: &mut State) -> Self
    {
        let dimensions = window.inner_size();

        let mut instance_desc = wgpu::InstanceDescriptor::default();

        if is_windows()
        {
            instance_desc.backends = wgpu::Backends::VULKAN;
            //instance_desc.backends = wgpu::Backends::DX12;
        }

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
        let adapter_info = adapter.get_info();
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
                //features: wgpu::Features::empty(),
                features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, // for multisampling
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

        let mut present_mode = wgpu::PresentMode::Fifo;
        if !state.rendering.v_sync.get_ref()
        {
            present_mode = wgpu::PresentMode::Immediate;
        }

        let surface_config = wgpu::SurfaceConfiguration
        {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            width: dimensions.width,
            height: dimensions.height,
            present_mode: present_mode,
            //alpha_mode: surface_caps.alpha_modes[0], //wgpu::CompositeAlphaMode::Auto
            alpha_mode: surface_caps.alpha_modes[0], //wgpu::CompositeAlphaMode::Auto
            format: surface_caps.formats[0],
            view_formats: vec![],
        };

        surface.configure(&device, &surface_config);

        // msaa
        let texture_features = adapter.get_texture_format_features(surface_caps.formats[0]);

        if texture_features.flags.sample_count_supported(2) { state.adapter.max_msaa_samples = 2; }
        if texture_features.flags.sample_count_supported(4) { state.adapter.max_msaa_samples = 4; }
        if texture_features.flags.sample_count_supported(8) { state.adapter.max_msaa_samples = 8; }
        if texture_features.flags.sample_count_supported(16) { state.adapter.max_msaa_samples = 16; }

        let msaa_samples = *state.rendering.msaa.get_ref();

        state.adapter.max_texture_resolution = device.limits().max_texture_dimension_2d;
        state.adapter.max_supported_texture_resolution = device.limits().max_texture_dimension_2d;

        // storage support
        let supports_storage_resources = adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) && device.limits().max_storage_buffers_per_shader_stage > 0;
        state.adapter.storage_buffer_array_support = supports_storage_resources;

        // apply adapter infos
        state.adapter.name = adapter_info.name.clone();
        state.adapter.driver = adapter_info.driver.clone();
        state.adapter.driver_info = adapter_info.driver_info.clone();

        match adapter_info.backend
        {
            wgpu::Backend::Empty => state.adapter.backend = "Empty".to_string(),
            wgpu::Backend::Vulkan => state.adapter.backend = "Vulkan".to_string(),
            wgpu::Backend::Metal => state.adapter.backend = "Metal".to_string(),
            wgpu::Backend::Dx12 => state.adapter.backend = "Dx12".to_string(),
            wgpu::Backend::Dx11 => state.adapter.backend = "Dx11".to_string(),
            wgpu::Backend::Gl => state.adapter.backend = "Gl".to_string(),
            wgpu::Backend::BrowserWebGpu => state.adapter.backend = "BrowserWebGpu".to_string(),
        }

        let mut wgpu = Self
        {
            device,
            surface,
            msaa_samples,
            msaa_texture: None,
            queue,
            surface_caps,
            surface_config
        };

        wgpu.create_msaa_texture(1);

        wgpu
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

    pub fn create_msaa_texture(&mut self, sample_count: u32)
    {
        self.msaa_samples = sample_count;

        if sample_count <= 1
        {
            self.msaa_texture = None;
            return;
        }

        let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor
        {
            label: Some("msaa_texture"),
            size: wgpu::Extent3d
            {
                width: self.surface_config.width,
                height: self.surface_config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: self.surface_config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &self.surface_config.view_formats,
        });

        self.msaa_texture = Some(msaa_texture);
    }

    pub fn resize(&mut self, width: u32, height: u32)
    {
        self.surface_config.width = width;
        self.surface_config.height = height;

        self.surface.configure(&self.device, &self.surface_config);
        self.create_msaa_texture(self.msaa_samples);
    }

    pub fn set_vsync(&mut self, v_sync: bool)
    {
        let mut present_mode = wgpu::PresentMode::Fifo;
        if !v_sync
        {
            present_mode = wgpu::PresentMode::Immediate;
        }

        self.surface_config.present_mode = present_mode;

        self.surface.configure(&self.device, &self.surface_config);
        self.create_msaa_texture(self.msaa_samples);
    }

    pub fn start_render(&mut self) -> (SurfaceTexture, TextureView, Option<TextureView>, CommandEncoder)
    {
        // TODO: this can timeout
        // thread 'main' panicked at 'called `Result::unwrap()` on an `Err` value: Timeout', src\rendering\wgpu.rs:200:57
        //let output = self.surface.get_current_texture().unwrap();

        let mut output: Result<wgpu::SurfaceTexture, wgpu::SurfaceError>;
        loop
        {
            output = self.surface.get_current_texture();

            if output.is_ok()
            {
                break;
            }

            dbg!(output.err());

            // wait on error and retry
            sleep_millis(100);
            println!("retry get surface texture");
        }
        let output = output.unwrap();

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let mut msaa_view = None;
        if self.msaa_texture.is_some()
        {
            msaa_view = Some(self.msaa_texture.as_ref().unwrap().create_view(&wgpu::TextureViewDescriptor::default()));
        }

        (output, view, msaa_view, encoder)
    }

    pub fn end_render(&mut self, output: SurfaceTexture, encoder: CommandEncoder)
    {
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn start_screenshot_render(&mut self) -> (BufferDimensions, Buffer, Texture, TextureView, Option<TextureView>, CommandEncoder)
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


        let mut msaa_texture_view: Option<TextureView> = None;
        if self.msaa_samples > 1
        {
            let msaa_texture = self.device.create_texture(&wgpu::TextureDescriptor
            {
                size: texture_extent,
                mip_level_count: 1,
                sample_count: self.msaa_samples,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                label: None,
                view_formats: &[],
            });

            msaa_texture_view = Some(msaa_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        }


        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        (buffer_dimensions, output_buffer, texture, view, msaa_texture_view, encoder)
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
