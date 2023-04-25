use wgpu::{Device, Queue, Surface, SurfaceCapabilities, SurfaceConfiguration, CommandEncoder, TextureView};

pub trait WGpuRendering
{
    fn render_pass(&mut self, wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder);
}

//pub type WGpuRenderingItem = dyn WGpuRendering + Send + Sync;
pub type WGpuRenderingItem = dyn WGpuRendering;

pub struct WGpu
{
    device: Device,
    queue: Queue,
    surface: Surface,

    /*
    square_pipeline: Pipeline,

    square_buffers: VertexBuffer,
    instance_buffers: InstanceBuffers,
    */

    surface_config: SurfaceConfiguration,
    pub surface_caps: SurfaceCapabilities,
}

impl WGpu
{
    pub async fn new(window: &winit::window::Window) -> Self
    {
        let dimensions = window.inner_size();

        let mut instance_desc = wgpu::InstanceDescriptor::default();
        //instance_desc.backends = wgpu::Backends::VULKAN;

        let instance = wgpu::Instance::new(instance_desc);
        let surface = unsafe { instance.create_surface(window) }.unwrap();

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions
        {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .unwrap();

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

    pub fn render(&mut self, render_passes: &mut Vec<&mut WGpuRenderingItem>)
    {
        let output = self.surface.get_current_texture().unwrap();

        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        for pass in render_passes
        {
            pass.render_pass(self, &view, &mut encoder);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
