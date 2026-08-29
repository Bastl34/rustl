use wgpu::{BindGroup, BindGroupLayout, CommandEncoder, Device, Queue, TextureView};

use crate::{resources::resources, state::state::State};

use super::{pipeline::Pipeline, texture, wgpu::WGpu};

/*
    post processing: the scene renders linear hdr into a Rgba16Float target, this module
    turns it into the final (ldr) image:

    1. bloom downsample chain: hdr -> mip 0 -> mip 1 -> ... (13 tap filter, the first pass
       uses a karis average against fireflies) - thresholdless (CoD: Advanced Warfare style)
    2. bloom upsample chain: additive 3x3 tent filter back up the chain (result in mip 0)
    3. composite pass: mix(hdr, bloom, intensity) -> exposure tone mapping -> gamma -> target
*/

// must match CompositeUniform in composite.wgsl
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CompositeUniform
{
    pub exposure: f32,        // 0.0 = tone mapping disabled
    pub gamma: f32,           // 0.0 = gamma correction disabled
    pub bloom_intensity: f32, // 0.0 = bloom disabled
    pub _padding: f32,
}

// bloom intensity of the global rendering settings - 0.0 when bloom is disabled, which
// makes the render skip the whole bloom mip chain
pub fn bloom_intensity(state: &State) -> f32
{
    if state.rendering.bloom { state.rendering.bloom_intensity } else { 0.0 }
}

// (exposure, gamma, bloom intensity) for the fullscreen composite pass
//
// exposure/gamma are taken from the first visible scene - they used to be applied per scene
// inside the object shader, but the composite is a single fullscreen pass over all scenes
pub fn post_process_params(state: &State) -> (f32, f32, f32)
{
    for scene in &state.scenes
    {
        if !scene.visible || !scene.active || scene.render_item.is_none()
        {
            continue;
        }

        let scene_data = scene.get_data();
        return (scene_data.exposure.unwrap_or(0.0), scene_data.gamma.unwrap_or(0.0), bloom_intensity(state));
    }

    (0.0, 0.0, bloom_intensity(state))
}

pub struct PostProcess
{
    pub width: u32,
    pub height: u32,

    bloom_mip_count: u32,

    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,

    bloom_bind_layout: BindGroupLayout,
    composite_bind_layout: BindGroupLayout,

    downsample_first_pipeline: Pipeline,
    downsample_pipeline: Pipeline,
    upsample_pipeline: Pipeline,
    composite_pipeline: Pipeline,

    // one view per bloom mip level (each is a render target and a sample source)
    bloom_texture: wgpu::Texture,
    bloom_mip_views: Vec<TextureView>,

    // [0]: hdr -> mip 0, [i]: mip i-1 -> mip i
    downsample_bind_groups: Vec<BindGroup>,

    // [i]: mip i+1 -> mip i (additive)
    upsample_bind_groups: Vec<BindGroup>,

    composite_bind_group: BindGroup,
}

impl PostProcess
{
    pub fn new(wgpu: &mut WGpu, hdr_view: &TextureView, width: u32, height: u32) -> PostProcess
    {
        let bloom_shader = resources::load_string("shader/bloom.wgsl").unwrap();
        let composite_shader = resources::load_string("shader/composite.wgsl").unwrap();

        let surface_format = wgpu.surface_config().format;

        let sampler;
        let uniform_buffer;
        let bloom_bind_layout;
        let composite_bind_layout;
        {
            let device = wgpu.device();

            // bloom samples between mips and at the half res border -> linear + clamp
            sampler = device.create_sampler(&wgpu::SamplerDescriptor
            {
                label: Some("post process sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor
            {
                label: Some("post process uniform"),
                size: std::mem::size_of::<CompositeUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            bloom_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
            {
                label: Some("bloom bind group layout"),
                entries:
                &[
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture
                        {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

            composite_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
            {
                label: Some("composite bind group layout"),
                entries:
                &[
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer
                        {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture
                        {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture
                        {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry
                    {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        }

        // additive blending for the upsample chain (accumulates into the bigger mip)
        let additive_blend = wgpu::BlendState
        {
            color: wgpu::BlendComponent
            {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent
            {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let downsample_first_pipeline = Pipeline::new_fullscreen(wgpu, "bloom downsample first", &bloom_shader, &[&bloom_bind_layout], texture::Texture::HDR_FORMAT, "fs_downsample_first", None, false);
        let downsample_pipeline = Pipeline::new_fullscreen(wgpu, "bloom downsample", &bloom_shader, &[&bloom_bind_layout], texture::Texture::HDR_FORMAT, "fs_downsample", None, false);
        let upsample_pipeline = Pipeline::new_fullscreen(wgpu, "bloom upsample", &bloom_shader, &[&bloom_bind_layout], texture::Texture::HDR_FORMAT, "fs_upsample", Some(additive_blend), false);
        let composite_pipeline = Pipeline::new_fullscreen(wgpu, "post process composite", &composite_shader, &[&composite_bind_layout], surface_format, "fs_main", None, false);

        let (bloom_texture, bloom_mip_count, bloom_mip_views) = Self::create_bloom_chain(wgpu.device(), width, height);
        let (downsample_bind_groups, upsample_bind_groups, composite_bind_group) = Self::create_bind_groups(wgpu.device(), &bloom_bind_layout, &composite_bind_layout, &sampler, &uniform_buffer, hdr_view, &bloom_mip_views);

        PostProcess
        {
            width,
            height,

            bloom_mip_count,

            sampler,
            uniform_buffer,

            bloom_bind_layout,
            composite_bind_layout,

            downsample_first_pipeline,
            downsample_pipeline,
            upsample_pipeline,
            composite_pipeline,

            bloom_texture,
            bloom_mip_views,

            downsample_bind_groups,
            upsample_bind_groups,
            composite_bind_group,
        }
    }

    // rebinds the post processing to a (possibly new) hdr scene color view - the pipelines
    // always survive, the bloom mip chain is only rebuilt when the resolution really changed
    // (offscreen renders reuse one instance at a constant size, so this is the common case)
    pub fn resize(&mut self, device: &Device, hdr_view: &TextureView, width: u32, height: u32)
    {
        if width != self.width || height != self.height
        {
            let (bloom_texture, bloom_mip_count, bloom_mip_views) = Self::create_bloom_chain(device, width, height);

            self.width = width;
            self.height = height;
            self.bloom_mip_count = bloom_mip_count;
            self.bloom_texture = bloom_texture;
            self.bloom_mip_views = bloom_mip_views;
        }

        // the bind groups reference the hdr view, so they are rebuilt either way
        let (downsample_bind_groups, upsample_bind_groups, composite_bind_group) = Self::create_bind_groups(device, &self.bloom_bind_layout, &self.composite_bind_layout, &self.sampler, &self.uniform_buffer, hdr_view, &self.bloom_mip_views);

        self.downsample_bind_groups = downsample_bind_groups;
        self.upsample_bind_groups = upsample_bind_groups;
        self.composite_bind_group = composite_bind_group;
    }

    fn create_bloom_chain(device: &Device, width: u32, height: u32) -> (wgpu::Texture, u32, Vec<TextureView>)
    {
        // the bloom chain starts at half resolution; enough mips for a wide glow,
        // but the smallest mip should not collapse below a few pixels
        let bloom_width = (width / 2).max(1);
        let bloom_height = (height / 2).max(1);

        let max_mips = (bloom_width.min(bloom_height) as f32).log2().floor() as u32;
        let bloom_mip_count = max_mips.saturating_sub(2).clamp(1, 7);

        let bloom_texture = device.create_texture(&wgpu::TextureDescriptor
        {
            label: Some("bloom texture"),
            size: wgpu::Extent3d
            {
                width: bloom_width,
                height: bloom_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: bloom_mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture::Texture::HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        // one single-mip view per level (render target and sample source of the chain passes)
        let mut bloom_mip_views: Vec<TextureView> = vec![];
        for mip in 0..bloom_mip_count
        {
            bloom_mip_views.push(bloom_texture.create_view(&wgpu::TextureViewDescriptor
            {
                label: Some(format!("bloom mip {} view", mip).as_str()),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            }));
        }

        (bloom_texture, bloom_mip_count, bloom_mip_views)
    }

    fn create_bind_groups(device: &Device, bloom_bind_layout: &BindGroupLayout, composite_bind_layout: &BindGroupLayout, sampler: &wgpu::Sampler, uniform_buffer: &wgpu::Buffer, hdr_view: &TextureView, bloom_mip_views: &Vec<TextureView>) -> (Vec<BindGroup>, Vec<BindGroup>, BindGroup)
    {
        let bloom_mip_count = bloom_mip_views.len();

        let bloom_bind_group = |source: &TextureView, name: &str| -> BindGroup
        {
            device.create_bind_group(&wgpu::BindGroupDescriptor
            {
                label: Some(name),
                layout: bloom_bind_layout,
                entries:
                &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };

        // downsample: [0] samples the hdr scene color, [i] samples the previous mip
        let mut downsample_bind_groups: Vec<BindGroup> = vec![];
        downsample_bind_groups.push(bloom_bind_group(hdr_view, "bloom downsample bind group 0"));
        for mip in 1..bloom_mip_count
        {
            downsample_bind_groups.push(bloom_bind_group(&bloom_mip_views[mip - 1], format!("bloom downsample bind group {}", mip).as_str()));
        }

        // upsample: [i] samples mip i+1 (and renders additively into mip i)
        let mut upsample_bind_groups: Vec<BindGroup> = vec![];
        for mip in 0..(bloom_mip_count - 1)
        {
            upsample_bind_groups.push(bloom_bind_group(&bloom_mip_views[mip + 1], format!("bloom upsample bind group {}", mip).as_str()));
        }

        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor
        {
            label: Some("composite bind group"),
            layout: composite_bind_layout,
            entries:
            &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(hdr_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&bloom_mip_views[0]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });

        (downsample_bind_groups, upsample_bind_groups, composite_bind_group)
    }

    // bloom chain + composite: hdr scene color -> target (usually the surface)
    pub fn render(&self, queue: &Queue, encoder: &mut CommandEncoder, target_view: &TextureView, exposure: f32, gamma: f32, bloom_intensity: f32)
    {
        let uniform = CompositeUniform
        {
            exposure,
            gamma,
            bloom_intensity,
            _padding: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));

        // ********** bloom mip chain **********
        if bloom_intensity > 0.0001
        {
            // downsample: hdr -> mip 0 -> mip 1 -> ...
            for mip in 0..self.bloom_mip_count as usize
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
                {
                    label: Some("bloom downsample pass"),
                    color_attachments:
                    &[
                        Some(wgpu::RenderPassColorAttachment
                        {
                            view: &self.bloom_mip_views[mip],
                            resolve_target: None,
                            ops: wgpu::Operations
                            {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                let pipeline = if mip == 0 { &self.downsample_first_pipeline } else { &self.downsample_pipeline };

                pass.set_pipeline(pipeline.get());
                pass.set_bind_group(0, &self.downsample_bind_groups[mip], &[]);
                pass.draw(0..3, 0..1); // fullscreen triangle
            }

            // upsample: additive tent filter back up the chain (result in mip 0)
            for mip in (0..self.bloom_mip_count as usize - 1).rev()
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
                {
                    label: Some("bloom upsample pass"),
                    color_attachments:
                    &[
                        Some(wgpu::RenderPassColorAttachment
                        {
                            view: &self.bloom_mip_views[mip],
                            resolve_target: None,
                            ops: wgpu::Operations
                            {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })
                    ],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                pass.set_pipeline(self.upsample_pipeline.get());
                pass.set_bind_group(0, &self.upsample_bind_groups[mip], &[]);
                pass.draw(0..3, 0..1); // fullscreen triangle
            }
        }

        // ********** composite (hdr + bloom -> tone mapping -> gamma -> target) **********
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("post process composite pass"),
                color_attachments:
                &[
                    Some(wgpu::RenderPassColorAttachment
                    {
                        view: target_view,
                        resolve_target: None,
                        ops: wgpu::Operations
                        {
                            // the fullscreen triangle overwrites every pixel anyway
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(self.composite_pipeline.get());
            pass.set_bind_group(0, &self.composite_bind_group, &[]);
            pass.draw(0..3, 0..1); // fullscreen triangle
        }
    }
}
