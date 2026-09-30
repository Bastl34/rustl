use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Rgba};
use wgpu::{Device, Queue, Surface, SurfaceConfiguration, CommandEncoder, TextureView, SurfaceTexture, Buffer, Texture};

use crate::{console_error, console_log, helper::{image::brga_to_rgba, platform::is_windows}, state::state::{PresentModeSetting, State}};

use super::helper::buffer::{BufferDimensions, remove_padding};

fn resolve_present_mode(setting: PresentModeSetting, supports_mailbox: bool) -> wgpu::PresentMode
{
    match setting
    {
        PresentModeSetting::VSync => wgpu::PresentMode::AutoVsync,
        PresentModeSetting::FastVSync =>
        {
            if supports_mailbox { wgpu::PresentMode::Mailbox } else { wgpu::PresentMode::AutoVsync }
        },
        PresentModeSetting::VSyncOff => wgpu::PresentMode::AutoNoVsync,
    }
}

pub struct WGpu
{
    device: Device,
    queue: Queue,
    surface: Surface<'static>,

    msaa_samples: u32,
    msaa_texture: Option<wgpu::Texture>,

    surface_config: SurfaceConfiguration,
    supports_mailbox: bool,
}

impl WGpu
{
    pub async fn new(window: Arc<winit::window::Window>, state: &mut State) -> Self
    {
        let dimensions = window.inner_size();

        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();

        // disable debug/validation flags — DX12 debug layer requires Windows "Graphics Tools"
        // component to be installed, otherwise request_device fails with Device(Lost)
        instance_desc.flags = wgpu::InstanceFlags::empty();

        if is_windows()
        {
            instance_desc.backends = wgpu::Backends::VULKAN | wgpu::Backends::DX12;
            // FXC: ships with Windows, no extra DLLs needed. Switch to StaticDxc once MSVC >= 14.40
            // TODO: check if this is still needed with newer MSVC versions
            instance_desc.backend_options.dx12.shader_compiler = wgpu::Dx12Compiler::Fxc;
        }

        let instance = wgpu::Instance::new(instance_desc);
        //let surface = unsafe { instance.create_surface(window) }.unwrap();
        let surface = instance.create_surface(window.clone());

        if let Err(surface_error) = &surface
        {
            console_error!(surface_error);
            panic!("Failed to create surface");
        }

        let surface = surface.unwrap();

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions
        {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            // no limit bucketing: this is a trusted native/wasm app, so use the adapter's real limits
            apply_limit_buckets: false,
        })
        .await
        .unwrap();

        console_log!(" ********** info **********");
        let adapter_info = adapter.get_info();
        console_log!(adapter.get_info());

        console_log!(" ********** features possible **********");
        console_log!(adapter.features());

        console_log!(" ********** limits possible **********");
        console_log!(adapter.limits());

        let adapter_features = adapter.features();
        let polygon_mode_features = wgpu::Features::POLYGON_MODE_LINE | wgpu::Features::POLYGON_MODE_POINT;
        let supported_polygon_mode_features = adapter_features & polygon_mode_features;
        let timestamp_query_features = adapter_features & wgpu::Features::TIMESTAMP_QUERY;

        let device_result = adapter.request_device
        (
            &wgpu::DeviceDescriptor
            {
                label: None,
                required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | supported_polygon_mode_features | timestamp_query_features, // for multisampling + wireframe + gpu timing (if supported)
                // WebGL doesn't support all of wgpu's features, so if building for the web: disable some
                required_limits: if cfg!(target_arch = "wasm32")
                {
                    wgpu::Limits::downlevel_webgl2_defaults()
                }
                else
                {
                    adapter.limits()
                },
                memory_hints: Default::default(),
                experimental_features: Default::default(),
                trace: wgpu::Trace::Off,
            },
        )
        .await;

        let (device, queue) = match device_result
        {
            Ok(dq) => dq,
            Err(err) =>
            {
                console_error!(format!("request_device failed: {:?}", err));
                console_error!("retrying with minimal features and downlevel limits");

                adapter.request_device
                (
                    &wgpu::DeviceDescriptor
                    {
                        label: None,
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::downlevel_defaults(),
                        memory_hints: Default::default(),
                        experimental_features: Default::default(),
                        trace: wgpu::Trace::Off,
                    },
                )
                .await
                .expect("request_device failed even with minimal config")
            }
        };

        console_log!(" ********** features used **********");
        console_log!(device.features());

        console_log!(" ********** limits used **********");
        console_log!(device.limits());

        let surface_caps = surface.get_capabilities(&adapter);

        let supports_mailbox = surface_caps.present_modes.contains(&wgpu::PresentMode::Mailbox);

        let present_mode = resolve_present_mode(*state.rendering.present_mode.get_ref(), supports_mailbox);

        let surface_config = wgpu::SurfaceConfiguration
        {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            width: dimensions.width,
            height: dimensions.height,
            present_mode: present_mode,
            alpha_mode: surface_caps.alpha_modes[0], //wgpu::CompositeAlphaMode::Auto
            format: surface_caps.formats[0],
            // Auto reproduces the pre-wgpu-30 behaviour (sRGB, or extended linear sRGB for fp16 surfaces)
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1, // 1: lower latency, 2: higher throughput maybe check https://github.com/emilk/egui/blob/main/crates/egui-wgpu/src/lib.rs#L331 for ios issues
        };

        surface.configure(&device, &surface_config);

        // msaa
        let texture_features = adapter.get_texture_format_features(surface_caps.formats[0]);

        if texture_features.flags.sample_count_supported(2) { state.rendering_adapter.max_msaa_samples = 2; }
        if texture_features.flags.sample_count_supported(4) { state.rendering_adapter.max_msaa_samples = 4; }
        if texture_features.flags.sample_count_supported(8) { state.rendering_adapter.max_msaa_samples = 8; }
        if texture_features.flags.sample_count_supported(16) { state.rendering_adapter.max_msaa_samples = 16; }

        let msaa_samples = *state.rendering.msaa.get_ref();

        state.rendering_adapter.max_texture_resolution = device.limits().max_texture_dimension_2d;
        state.rendering_adapter.max_supported_texture_resolution = device.limits().max_texture_dimension_2d;

        // storage support
        let supports_storage_resources = adapter.get_downlevel_capabilities().flags.contains(wgpu::DownlevelFlags::VERTEX_STORAGE) && device.limits().max_storage_buffers_per_shader_stage > 0;
        state.rendering_adapter.storage_buffer_array_support = supports_storage_resources;

        // wireframe support
        state.rendering_adapter.wireframe_mode_support = device.features().contains(wgpu::Features::POLYGON_MODE_LINE);

        // occlusion culling support: the hzb culling needs compute shaders, indirect draws
        // and r32float storage texture writes (all missing on WebGL)
        let downlevel_flags = adapter.get_downlevel_capabilities().flags;
        let r32_float_usages = adapter.get_texture_format_features(wgpu::TextureFormat::R32Float).allowed_usages;
        state.rendering_adapter.occlusion_culling_support =
            downlevel_flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
            && downlevel_flags.contains(wgpu::DownlevelFlags::INDIRECT_EXECUTION)
            && r32_float_usages.contains(wgpu::TextureUsages::STORAGE_BINDING);

        // ssao support: the ssao shader does textureLoad on a depth texture, which naga's
        // GLSL backend does not support -> the ssao pipelines must not even be created there
        state.rendering_adapter.ssao_support = adapter_info.backend != wgpu::Backend::Gl;

        // apply adapter infos
        state.rendering_adapter.name = adapter_info.name.clone();
        state.rendering_adapter.driver = adapter_info.driver.clone();
        state.rendering_adapter.driver_info = adapter_info.driver_info.clone();

        match adapter_info.backend
        {
            wgpu::Backend::Noop => state.rendering_adapter.backend = "Noop".to_string(),
            wgpu::Backend::Vulkan => state.rendering_adapter.backend = "Vulkan".to_string(),
            wgpu::Backend::Metal => state.rendering_adapter.backend = "Metal".to_string(),
            wgpu::Backend::Dx12 => state.rendering_adapter.backend = "Dx12".to_string(),
            wgpu::Backend::Gl => state.rendering_adapter.backend = "Gl".to_string(),
            wgpu::Backend::BrowserWebGpu => state.rendering_adapter.backend = "BrowserWebGpu".to_string(),
        }

        let mut wgpu = Self
        {
            device,
            surface,
            msaa_samples,
            msaa_texture: None,
            queue,
            surface_config,
            supports_mailbox,
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

    pub fn set_present_mode(&mut self, setting: PresentModeSetting)
    {
        self.surface_config.present_mode = resolve_present_mode(setting, self.supports_mailbox);

        self.surface.configure(&self.device, &self.surface_config);
        self.create_msaa_texture(self.msaa_samples);
    }

    pub fn start_render(&mut self) -> Option<(SurfaceTexture, TextureView, Option<TextureView>)>
    {
        let output = match self.surface.get_current_texture()
        {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Occluded => return None,
            other =>
            {
                console_error!(format!("{:?}", other));
                return None;
            }
        };

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut msaa_view = None;
        if self.msaa_texture.is_some()
        {
            msaa_view = Some(self.msaa_texture.as_ref().unwrap().create_view(&wgpu::TextureViewDescriptor::default()));
        }

        Some((output, view, msaa_view))
    }

    pub fn create_command_encoder(&mut self) -> CommandEncoder
    {
        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
    }

    pub fn submit_commands(&mut self, encoders: Vec<CommandEncoder>)
    {
        let command_buffers: Vec<wgpu::CommandBuffer> = encoders
            .into_iter()
            .map(|encoder| encoder.finish())
            .collect();

        self.queue.submit(command_buffers);
    }

    pub fn end_render(&mut self, output: SurfaceTexture)
    {
        self.queue.present(output);
    }


    pub fn start_offscreen_render(&mut self, resolution: Option<(u32, u32)>) -> (BufferDimensions, Buffer, Texture, TextureView, Option<TextureView>)
    {
        // when a custom resolution is given the scene must be rendered natively at that size
        // (the caller is responsible for resizing the scene's depth buffer / camera / hzb accordingly).
        let (width, height) = resolution.unwrap_or((self.surface_config.width, self.surface_config.height));
        let buffer_dimensions = BufferDimensions::new(width as usize, height as usize);

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
            //format: wgpu::TextureFormat::Rgba8UnormSrgb,
            format: self.surface_config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            label: None,
            //view_formats: &[],
            view_formats: &self.surface_config.view_formats,
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
                //format: wgpu::TextureFormat::Rgba8UnormSrgb,
                format: self.surface_config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                label: None,
                //view_formats: &[],
                view_formats: &self.surface_config.view_formats,
            });

            msaa_texture_view = Some(msaa_texture.create_view(&wgpu::TextureViewDescriptor::default()));
        }


        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (buffer_dimensions, output_buffer, texture, view, msaa_texture_view)
    }

    pub fn end_offscreen_render(&mut self, buffer_dimensions: BufferDimensions, output_buffer: Buffer, texture: Texture, mut encoder: CommandEncoder) -> DynamicImage
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
            wgpu::TexelCopyBufferInfo
            {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout
                {
                    offset: 0,
                    bytes_per_row: Some(buffer_dimensions.padded_bytes_per_row as u32),
                    rows_per_image: None,
                },
            },
            texture_extent,
        );

        self.submit_commands(vec![encoder]);

        // read buffer
        let slice: wgpu::BufferSlice = output_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| ());
        self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();

        // remove padding
        let padded_data = slice.get_mapped_range().unwrap();
        let data = remove_padding(&padded_data, &buffer_dimensions);
        drop(padded_data);

        output_buffer.unmap();

        let img = DynamicImage::ImageRgba8(ImageBuffer::<Rgba<u8>, _>::from_raw(buffer_dimensions.width as u32, buffer_dimensions.height as u32, data).unwrap());
        brga_to_rgba(img)
    }
}
