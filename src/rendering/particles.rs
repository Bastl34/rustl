use std::{borrow::Cow, collections::HashMap};

use nalgebra::{Matrix4, Point3, Vector3};
use wgpu::{BindGroup, BindGroupLayout, CommandEncoder, TextureView};

use crate::{rendering::{bind_groups::uniform, camera::CameraBuffer, scene::FogUniform, shadow::{self, ShadowBuffer, ShadowViewData}, texture::{Texture, TextureFormat}, wgpu::WGpu}, state::{helper::render_item::get_render_item, resources::texture::TextureItem, scene::{camera::CameraData, components::particle_emitter::{ParticleAlign, ParticleBlend, ParticleEmitter}, node::{Node, NodeItem}, scene::Scene}}};

const MIN_INSTANCES: usize = 256; // per emitter, the buffer grows on demand

// keep in sync with particles.wgsl
const FLAG_TEXTURED: u32 = 1;
const FLAG_FOG: u32 = 2;
const FLAG_ADDITIVE: u32 = 4;
const ALIGN_SHIFT: u32 = 4; // 0 billboard, 1 velocity, 2 node

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ParticleInstance
{
    pub position: [f32; 3], // world space
    pub size: f32,
    pub color: [f32; 4],
    pub rotation: f32,
    pub frame: u32,
    pub flipbook: u32, // columns | rows << 16
    pub flags: u32,
    pub direction: [f32; 3], // velocity (align Velocity) or the plane normal (align Node), world space
    pub stretch: f32,
    pub softness: f32, // m, 0: off
}

const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array!
[
    0 => Float32x3,
    1 => Float32,
    2 => Float32x4,
    3 => Float32,
    4 => Uint32,
    5 => Uint32,
    6 => Uint32,
    7 => Float32x3,
    8 => Float32,
    9 => Float32,
];

// per camera: the depth of its projection back to view space (soft particles)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DepthData
{
    projection: [f32; 4], // m22, m23, m32, m33 of the projection
}

// the fog of the scene, like in the material shader
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FogData
{
    color: [f32; 4], // rgb, density
    params: [f32; 4], // start, end
}

struct InstanceBuffer
{
    camera_id: Option<u32>, // sorted for this camera, None: unsorted for all
    buffer: wgpu::Buffer,
    capacity: usize,
}

// the particles of one emitter
struct ParticleBatch
{
    buffers: Vec<InstanceBuffer>,
    count: u32,
    blend: ParticleBlend,
    texture: Option<u32>, // texture id
    center: Vector3<f32>, // world position of the node, orders the emitters per camera
    cameras: Vec<u32>, // the ones that see it
    shadow_bounds: Option<(Point3<f32>, f32)>, // cast_shadows: sphere of the particles, culled per shadow view
    translucent: bool, // translucent shadows (if the rendering setting is on)
}

impl ParticleBatch
{
    fn buffer(&self, camera_id: u32) -> Option<&wgpu::Buffer>
    {
        self.buffers.iter().find(|buffer| buffer.camera_id == Some(camera_id)).or_else(|| self.buffers.iter().find(|buffer| buffer.camera_id.is_none())).map(|buffer| &buffer.buffer)
    }
}

pub struct ParticleRenderer
{
    shader: String,
    shadow_shader: String,

    camera_layout: BindGroupLayout,
    texture_layout: BindGroupLayout,
    sampler: wgpu::Sampler,
    fog_buffer: wgpu::Buffer,

    pipelines: Option<(wgpu::RenderPipeline, wgpu::RenderPipeline)>, // alpha, additive
    shadow_pipeline: Option<wgpu::RenderPipeline>,
    translucent_pipeline: Option<wgpu::RenderPipeline>,
    pub translucent_shadows: bool, // the rendering setting

    camera_bind_groups: HashMap<u32, (BindGroup, wgpu::Buffer)>, // + depth uniform
    scene_depth: Option<wgpu::Texture>, // the depth pre-pass the bind groups were made for
    textures: HashMap<u32, (wgpu::Texture, BindGroup)>, // texture id: the gpu texture of the resource the bind group was made for
    untextured: (Texture, BindGroup),

    batches: Vec<ParticleBatch>,

    // emitter nodes, searched again when the node or component count changes
    emitter_nodes: Vec<NodeItem>,
    search_key: (usize, usize),
}

impl ParticleRenderer
{
    pub fn new(wgpu: &mut WGpu, shader: String, shadow_shader: String) -> Self
    {
        let camera_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                uniform::uniform_bind_group_layout_entry(0, true, false),
                uniform::uniform_bind_group_layout_entry(1, true, true),
                uniform::uniform_bind_group_layout_entry(2, false, true),

                // depth pre-pass of the scene (soft particles)
                wgpu::BindGroupLayoutEntry
                {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Depth,
                    },
                    count: None,
                },
            ],
            label: Some("particles camera bind group layout"),
        });

        let texture_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture
                    {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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
            label: Some("particles texture bind group layout"),
        });

        let sampler = wgpu.device().create_sampler(&wgpu::SamplerDescriptor
        {
            label: Some("particles sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let fog_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("particles fog"),
            size: std::mem::size_of::<FogData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let untextured_texture = Texture::new_empty_texture(wgpu, "particles untextured", TextureFormat::Srgba);
        let untextured_bind_group = Self::texture_bind_group(wgpu, &texture_layout, &sampler, &untextured_texture);

        Self
        {
            shader,
            shadow_shader,

            camera_layout,
            texture_layout,
            sampler,
            fog_buffer,

            pipelines: None,
            shadow_pipeline: None,
            translucent_pipeline: None,
            translucent_shadows: false,

            camera_bind_groups: HashMap::new(),
            scene_depth: None,
            textures: HashMap::new(),
            untextured: (untextured_texture, untextured_bind_group),

            batches: vec![],

            emitter_nodes: vec![],
            search_key: (usize::MAX, 0),
        }
    }

    fn texture_bind_group(wgpu: &mut WGpu, layout: &BindGroupLayout, sampler: &wgpu::Sampler, texture: &Texture) -> BindGroup
    {
        wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout,
            entries: &Texture::get_bind_group_entries(0, texture.get_view(), sampler),
            label: Some("particles texture bind group"),
        })
    }

    pub fn gpu_usage(&self) -> u64
    {
        self.batches.iter().flat_map(|batch| batch.buffers.iter()).map(|buffer| buffer.buffer.size()).sum()
    }

    // ********** pipelines **********

    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, samples: u32, reverse_z: bool)
    {
        let device = wgpu.device();
        let surface_format = wgpu.surface_config().format;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&self.shader)),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some("particles layout"),
            bind_group_layouts: &[Some(&self.camera_layout), Some(&self.texture_layout)],
            ..Default::default()
        });

        let create_pipeline = |name: &str, color: wgpu::BlendComponent, alpha: wgpu::BlendComponent| -> wgpu::RenderPipeline
        {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
            {
                label: Some(name),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState
                {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers:
                    &[
                        Some(wgpu::VertexBufferLayout
                        {
                            array_stride: std::mem::size_of::<ParticleInstance>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &INSTANCE_ATTRIBUTES,
                        })
                    ],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState
                {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState
                    {
                        format: surface_format,
                        blend: Some(wgpu::BlendState { color, alpha }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState
                {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                // tested against the scene depth, never written
                depth_stencil: Some(wgpu::DepthStencilState
                {
                    format: Texture::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(if reverse_z { wgpu::CompareFunction::GreaterEqual } else { wgpu::CompareFunction::LessEqual }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState
                {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };

        let alpha_blend = create_pipeline
        (
            "particles alpha",
            wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha },
            wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha },
        );

        // fades by alpha too - the target alpha stays
        let additive = create_pipeline
        (
            "particles additive",
            wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One },
            wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One },
        );

        self.pipelines = Some((alpha_blend, additive));

        self.create_shadow_pipeline(wgpu);
    }

    // depth only into a shadow atlas layer, like the shadow pipeline of the meshes
    fn create_shadow_pipeline(&mut self, wgpu: &mut WGpu)
    {
        let caster_layout = ShadowBuffer::caster_bind_layout(wgpu);
        let device = wgpu.device();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor
        {
            label: Some("particles shadow"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(&self.shadow_shader)),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor
        {
            label: Some("particles shadow layout"),
            bind_group_layouts: &[Some(&caster_layout), Some(&self.texture_layout)],
            ..Default::default()
        });

        let instance_layout = [Some(wgpu::VertexBufferLayout
        {
            array_stride: std::mem::size_of::<ParticleInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        })];

        // no depth: particles behind a mesh only darken what its shadow covers anyway
        self.translucent_pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some("particles translucent shadow"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &instance_layout, compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState
            {
                module: &shader,
                entry_point: Some("fs_translucent"),
                targets: &[Some(wgpu::ColorTargetState
                {
                    format: shadow::TRANSLUCENT_FORMAT,
                    blend: Some(wgpu::BlendState
                    {
                        color: wgpu::BlendComponent { operation: wgpu::BlendOperation::Add, src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::Src },
                        alpha: wgpu::BlendComponent { operation: wgpu::BlendOperation::Min, src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            multiview_mask: None,
            cache: None,
        }));

        self.shadow_pipeline = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor
        {
            label: Some("particles shadow"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState
            {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers:
                &[
                    Some(wgpu::VertexBufferLayout
                    {
                        array_stride: std::mem::size_of::<ParticleInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &INSTANCE_ATTRIBUTES,
                    })
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState
            {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState
            {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState
            {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState { count: 1, mask: !0, alpha_to_coverage_enabled: false },
            multiview_mask: None,
            cache: None,
        }));
    }

    // ********** update **********

    // a new camera buffer needs a new bind group
    // a new camera buffer or a new depth pre-pass texture (resize) needs a new bind group
    pub fn update_camera(&mut self, wgpu: &mut WGpu, camera_id: u32, camera_buffer: &CameraBuffer, buffer_created: bool, scene_depth: &Texture)
    {
        if self.scene_depth.as_ref() != Some(scene_depth.get_texture())
        {
            self.scene_depth = Some(scene_depth.get_texture().clone());
            self.camera_bind_groups.clear();
        }

        if !buffer_created && self.camera_bind_groups.contains_key(&camera_id)
        {
            return;
        }

        let depth_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("particles depth"),
            size: std::mem::size_of::<DepthData>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &self.camera_layout,
            entries:
            &[
                wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.get_buffer().as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.fog_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: depth_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(scene_depth.get_view()) },
            ],
            label: Some("particles camera bind group"),
        });

        self.camera_bind_groups.insert(camera_id, (bind_group, depth_buffer));
    }

    pub fn remove_unused_cameras(&mut self, scene: &Scene)
    {
        self.camera_bind_groups.retain(|id, _| scene.cameras.iter().any(|cam| cam.id == *id));
    }

    // the particles of every emitter into its instance buffer - cameras: the active ones
    pub fn update(&mut self, wgpu: &mut WGpu, scene: &Scene, all_nodes: &Vec<NodeItem>, cameras: &[u32], fog: &FogUniform, reverse_z: bool)
    {
        let fog_data = FogData { color: [fog.color.x, fog.color.y, fog.color.z, fog.density], params: [fog.start, fog.end, 0.0, 0.0] };
        wgpu.queue_mut().write_buffer(&self.fog_buffer, 0, bytemuck::bytes_of(&fog_data));

        for camera in scene.cameras.iter().filter(|camera| cameras.contains(&camera.id))
        {
            if let Some((_, depth_buffer)) = self.camera_bind_groups.get(&camera.id)
            {
                let projection = camera.webgpu_projection(reverse_z);
                let depth_data = DepthData { projection: [projection[(2, 2)], projection[(2, 3)], projection[(3, 2)], projection[(3, 3)]] };
                wgpu.queue_mut().write_buffer(depth_buffer, 0, bytemuck::bytes_of(&depth_data));
            }
        }

        let component_count = all_nodes.iter().map(|node| node.read().unwrap().components.len()).sum();
        let search_key = (all_nodes.len(), component_count);
        if search_key != self.search_key
        {
            self.search_key = search_key;
            self.emitter_nodes = all_nodes.iter().filter(|node| !node.read().unwrap().find_components::<ParticleEmitter>().is_empty()).cloned().collect();
        }

        let mut used = 0;
        let emitter_nodes = self.emitter_nodes.clone();

        for node_item in &emitter_nodes
        {
            // before the lock below - it locks the node itself
            let node_alpha = Node::get_full_alpha(node_item.clone());

            let node = node_item.read().unwrap();
            let shown = node.is_visible() && node_alpha > 0.0;

            let node_transform = node.get_full_transform();
            let center = node_transform.column(3).xyz();

            for component in node.find_components::<ParticleEmitter>()
            {
                let component = component.read().unwrap();
                let Some(emitter) = component.as_any().downcast_ref::<ParticleEmitter>() else { continue; };

                if !component.is_enabled()
                {
                    continue;
                }

                // bound before the first particle - the texture shows up in the editor right away
                let data = emitter.get_data();
                let texture = data.texture.as_ref().and_then(|texture| self.bind_texture(wgpu, texture));
                let transform = if data.world_space { Matrix4::identity() } else { node_transform };

                // culling: the particles and the node origin in a sphere - empty: as far as new particles get
                let (bounds_center, radius) = if emitter.particles().is_empty()
                {
                    (center, emitter.reach())
                }
                else
                {
                    let (min, max) = emitter.particles().iter().fold((center, center), |(min, max), particle|
                    {
                        let position = transform.transform_point(&Point3::from(particle.position)).coords;
                        (min.inf(&position), max.sup(&position))
                    });
                    let margin = data.size_start.max(data.size_end) + data.size_var;
                    ((min + max) * 0.5, (max - min).norm() * 0.5 + margin)
                };

                let visible_cameras: Vec<u32> = if !shown { vec![] } else
                {
                    scene.cameras.iter().filter(|camera| cameras.contains(&camera.id)).filter(|camera|
                    {
                        let far = data.max_distance > 0.0 && (camera.get_data().eye_pos.coords - bounds_center).norm() - radius > data.max_distance;
                        !far && camera.is_sphere_in_frustum(&Point3::from(bounds_center), radius)
                    }).map(|camera| camera.id).collect()
                };

                emitter.set_hidden(!cameras.is_empty() && visible_cameras.is_empty());

                if emitter.particles().is_empty() || visible_cameras.is_empty()
                {
                    continue;
                }

                let align = match data.align
                {
                    ParticleAlign::Billboard => 0,
                    ParticleAlign::Velocity => 1,
                    ParticleAlign::Node => 2,
                };

                let mut flags = align << ALIGN_SHIFT;
                if texture.is_some() { flags |= FLAG_TEXTURED; }
                if data.fog { flags |= FLAG_FOG; }
                if data.blend == ParticleBlend::Additive { flags |= FLAG_ADDITIVE; }

                let flipbook = data.flipbook[0].clamp(1, 0xffff) | (data.flipbook[1].clamp(1, 0xffff) << 16);
                let node_normal = node_transform.transform_vector(&Vector3::y()).try_normalize(1e-6).unwrap_or(Vector3::y());

                // trail segments: from point to point, stretched like align Velocity, without texture - fading out to the old end
                let trail_flags = (flags & !(3 << ALIGN_SHIFT) & !FLAG_TEXTURED) | (1 << ALIGN_SHIFT);

                let mut instances: Vec<ParticleInstance> = Vec::with_capacity(emitter.particles().len());
                for particle in emitter.particles()
                {
                    let mut color = emitter.particle_color(particle);
                    color.w *= node_alpha;

                    let size = emitter.particle_size(particle);
                    let position = transform.transform_point(&Point3::from(particle.position)).coords;

                    if data.trail && data.trail_length > 0.0
                    {
                        let mut newer = (position, particle.age);
                        for (point, age) in particle.trail.iter().rev()
                        {
                            let point = transform.transform_point(&Point3::from(*point)).coords;
                            let fade = (1.0 - (particle.age - newer.1) / data.trail_length).clamp(0.0, 1.0);

                            instances.push(ParticleInstance
                            {
                                position: newer.0.into(),
                                size: size * data.trail_width,
                                color: [color.x, color.y, color.z, color.w * fade],
                                rotation: 0.0,
                                frame: 0,
                                flipbook,
                                flags: trail_flags,
                                direction: (newer.0 - point).into(),
                                stretch: 1.0,
                                softness: data.softness,
                            });

                            newer = (point, *age);
                        }
                    }

                    let direction = match data.align
                    {
                        ParticleAlign::Billboard => Vector3::zeros(),
                        ParticleAlign::Velocity => transform.transform_vector(&particle.velocity),
                        ParticleAlign::Node => node_normal,
                    };

                    instances.push(ParticleInstance
                    {
                        position: position.into(),
                        size,
                        color: color.into(),
                        rotation: particle.rotation,
                        frame: emitter.particle_frame(particle),
                        flipbook,
                        flags,
                        direction: direction.into(),
                        stretch: data.stretch,
                        softness: data.softness,
                    });
                }

                // one list per camera, back to front - else one for all
                let lists: Vec<(Option<u32>, Vec<ParticleInstance>)> = if data.sort && data.blend == ParticleBlend::Alpha
                {
                    scene.cameras.iter().filter(|camera| visible_cameras.contains(&camera.id)).map(|camera| (Some(camera.id), sort_back_to_front(&instances, &camera.get_data().eye_pos.coords))).collect()
                }
                else
                {
                    vec![(None, instances)]
                };

                if used >= self.batches.len()
                {
                    self.batches.push(ParticleBatch { buffers: vec![], count: 0, blend: data.blend, texture: None, center, cameras: vec![], shadow_bounds: None, translucent: false });
                }

                let batch = &mut self.batches[used];
                batch.blend = data.blend;
                batch.texture = texture;
                batch.center = center;
                batch.cameras = visible_cameras;
                batch.shadow_bounds = data.cast_shadows.then_some((Point3::from(bounds_center), radius));
                batch.translucent = data.translucent_shadows;
                write_batch(wgpu, batch, lists);

                used += 1;
            }
        }

        self.batches.truncate(used);
    }

    // drawn with the gpu texture of the resource (not on the gpu twice) - None until it is uploaded
    fn bind_texture(&mut self, wgpu: &mut WGpu, texture: &TextureItem) -> Option<u32>
    {
        let item = texture.read().unwrap();
        let gpu_texture = get_render_item::<Texture>(item.render_item.as_ref()?);

        // a new gpu texture (changed image) needs a new bind group
        if self.textures.get(&item.id).is_none_or(|(bound, _)| bound != gpu_texture.get_texture())
        {
            let bind_group = Self::texture_bind_group(wgpu, &self.texture_layout, &self.sampler, &gpu_texture);
            self.textures.insert(item.id, (gpu_texture.get_texture().clone(), bind_group));
        }

        Some(item.id)
    }

    // ********** render **********

    pub fn has_particles(&self) -> bool
    {
        self.batches.iter().any(|batch| batch.count > 0)
    }

    fn texture_bind_group_of(&self, batch: &ParticleBatch) -> &BindGroup
    {
        batch.texture.and_then(|id| self.textures.get(&id)).map(|(_, bind_group)| bind_group).unwrap_or(&self.untextured.1)
    }

    // a batch casts into this view - translucent: the opacity pass, else the depth pass
    fn casts_into(&self, batch: &ParticleBatch, shadow_view: &ShadowViewData, translucent: bool) -> bool
    {
        let Some((center, radius)) = batch.shadow_bounds else { return false; };
        batch.count > 0 && (batch.translucent && self.translucent_shadows) == translucent && shadow_view.intersects_sphere(&center, radius)
    }

    pub fn has_translucent_shadows(&self, shadow_view: &ShadowViewData) -> bool
    {
        self.batches.iter().any(|batch| self.casts_into(batch, shadow_view, true))
    }

    // the emitters with cast_shadows into the shadow pass of this view (depth, or the translucent pass) - only the ones a camera sees
    pub fn draw_shadows<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, shadow_view: &ShadowViewData, caster_bind_group: &'a BindGroup, translucent: bool) -> u32
    {
        let pipeline = if translucent { self.translucent_pipeline.as_ref() } else { self.shadow_pipeline.as_ref() };
        let Some(pipeline) = pipeline else { return 0; };

        let mut draw_calls = 0;
        let mut pipeline_set = false;

        for batch in &self.batches
        {
            if !self.casts_into(batch, shadow_view, translucent)
            {
                continue;
            }

            // any of its buffers, the order does not matter for depth
            let Some(buffer) = batch.buffers.first() else { continue; };

            if !pipeline_set
            {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, caster_bind_group, &[shadow_view.layer * shadow::SHADOW_VIEW_UNIFORM_STRIDE as u32]);
                pipeline_set = true;
            }

            pass.set_bind_group(1, self.texture_bind_group_of(batch), &[]);
            pass.set_vertex_buffer(0, buffer.buffer.slice(..));
            pass.draw(0..6, 0..batch.count);
            draw_calls += 1;
        }

        draw_calls
    }

    // particles and emitters drawn this frame
    pub fn stats(&self) -> (u32, u32)
    {
        let drawn = self.batches.iter().filter(|batch| batch.count > 0);
        drawn.fold((0, 0), |(particles, emitters), batch| (particles + batch.count, emitters + 1))
    }

    // on top of the color pass, depth tested against the scene
    pub fn render(&self, view: &TextureView, msaa_view: &Option<TextureView>, depth_view: &TextureView, encoder: &mut CommandEncoder, cam_data: &CameraData, camera_id: u32) -> u32
    {
        let Some((alpha_blend, additive)) = self.pipelines.as_ref() else { return 0; };
        let Some((camera_bind_group, _)) = self.camera_bind_groups.get(&camera_id) else { return 0; };

        let (render_pass_view, resolve_target) = match msaa_view
        {
            Some(msaa_view) => (msaa_view, Some(view)),
            None => (view, None),
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("particles pass"),
            color_attachments:
            &[
                Some(wgpu::RenderPassColorAttachment
                {
                    view: render_pass_view,
                    resolve_target,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        let [x, y, width, height] = cam_data.viewport_px();
        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);
        render_pass.set_bind_group(0, camera_bind_group, &[]);

        let mut draw_calls = 0;

        // the emitters back to front as well
        let eye = cam_data.eye_pos.coords;
        let mut batches: Vec<&ParticleBatch> = self.batches.iter().filter(|batch| batch.count > 0 && batch.cameras.contains(&camera_id)).collect();
        batches.sort_by(|a, b| (b.center - eye).norm_squared().total_cmp(&(a.center - eye).norm_squared()));

        for batch in batches
        {
            let Some(buffer) = batch.buffer(camera_id) else { continue; };

            let pipeline = match batch.blend
            {
                ParticleBlend::Alpha => alpha_blend,
                ParticleBlend::Additive => additive,
            };

            let texture_bind_group = self.texture_bind_group_of(batch);

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(1, texture_bind_group, &[]);
            render_pass.set_vertex_buffer(0, buffer.slice(..));
            render_pass.draw(0..6, 0..batch.count);
            draw_calls += 1;
        }

        draw_calls
    }
}

fn sort_back_to_front(instances: &[ParticleInstance], eye: &Vector3<f32>) -> Vec<ParticleInstance>
{
    let mut sorted: Vec<(f32, ParticleInstance)> = instances.iter().map(|instance| ((Vector3::from(instance.position) - eye).norm_squared(), *instance)).collect();
    sorted.sort_by(|a, b| b.0.total_cmp(&a.0));
    sorted.into_iter().map(|(_, instance)| instance).collect()
}

// one buffer per list - kept and only grown, the ones of cameras that are gone are dropped
fn write_batch(wgpu: &mut WGpu, batch: &mut ParticleBatch, lists: Vec<(Option<u32>, Vec<ParticleInstance>)>)
{
    batch.buffers.retain(|buffer| lists.iter().any(|(camera_id, _)| *camera_id == buffer.camera_id));
    batch.count = lists.first().map(|(_, instances)| instances.len() as u32).unwrap_or(0);

    for (camera_id, instances) in lists
    {
        let needed = instances.len().next_power_of_two().max(MIN_INSTANCES);

        let index = match batch.buffers.iter().position(|buffer| buffer.camera_id == camera_id)
        {
            Some(index) => index,
            None =>
            {
                batch.buffers.push(InstanceBuffer { camera_id, buffer: create_instance_buffer(wgpu, needed), capacity: needed });
                batch.buffers.len() - 1
            },
        };

        let buffer = &mut batch.buffers[index];
        if buffer.capacity < needed
        {
            buffer.buffer = create_instance_buffer(wgpu, needed);
            buffer.capacity = needed;
        }

        wgpu.queue_mut().write_buffer(&buffer.buffer, 0, bytemuck::cast_slice(&instances));
    }
}

fn create_instance_buffer(wgpu: &mut WGpu, capacity: usize) -> wgpu::Buffer
{
    wgpu.device().create_buffer(&wgpu::BufferDescriptor
    {
        label: Some("particles instance buffer"),
        size: (std::mem::size_of::<ParticleInstance>() * capacity) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
