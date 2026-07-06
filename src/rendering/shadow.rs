// Shadow mapping (shadow atlas)
//
// All shadow views (directional cascades, spot, point cube faces) share one Depth32Float
// texture array ("atlas"). The caster pass renders each view into its own layer using a
// single pipeline and a dynamic-offset uniform (see shader/shadow.wgsl). The color pass
// samples the atlas with a comparison sampler (hardware PCF) - see shader/base.wgsl.
//
// Layer assignment per light (in light order, capped by MAX_SHADOW_VIEWS):
// - directional: SHADOW_CASCADES layers (cascaded shadow maps, fitted to the first enabled camera)
// - spot: 1 layer
// - point: 6 layers (cube faces +X, -X, +Y, -Y, +Z, -Z)
// - hemispheric: no shadow
//
// The same assignment logic is used by LightBuffer (shadow_index in LightUniform) and by
// compute_shadow_views() - both must stay in sync (single source of truth: assign_shadow_views).

use std::cell::RefCell;

use nalgebra::{Isometry3, Matrix4, Orthographic3, Perspective3, Point3, Vector3, Vector4};

use crate::{helper::{change_tracker::ChangeTracker, math::up_vector_for_direction}, render_item_impl_default, state::{helper::render_item::RenderItem, scene::{camera::{CameraData, OPENGL_TO_WGPU_MATRIX}, light::{Light, LightItem, LightType}}}};

use super::wgpu::WGpu;

pub const MAX_SHADOW_VIEWS: u32 = 16;
pub const SHADOW_CASCADES: u32 = 3;

// resolution comes from state.rendering.shadow_map_resolution - this is just the lower sanity bound
const MIN_SHADOW_MAP_SIZE: u32 = 16;

// how far casters behind a cascade slice are still captured (multiple of the cascade radius)
const SHADOW_CSM_Z_EXTENSION: f32 = 4.0;

// blend factor between logarithmic and uniform cascade splits (practical split scheme)
const SHADOW_CSM_SPLIT_LAMBDA: f32 = 0.75;

// dynamic offset stride for the caster pass uniform (min_uniform_buffer_offset_alignment is <= 256 everywhere)
pub const SHADOW_VIEW_UNIFORM_STRIDE: u64 = 256;

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowViewUniform
{
    view_proj: [[f32; 4]; 4],
}

pub enum ShadowCull
{
    // spot/point: everything within radius around center casts into this view
    Sphere { center: Point3<f32>, radius: f32 },

    // directional cascade: infinite beam along dir with the cascade radius
    Beam { center: Point3<f32>, radius: f32, dir: Vector3<f32> },

    // no culling possible - render everything
    None,
}

pub struct ShadowViewData
{
    pub layer: u32,
    pub view_proj: Matrix4<f32>,
    pub cull: ShadowCull,
}

impl ShadowViewData
{
    pub fn intersects_sphere(&self, center: &Point3<f32>, radius: f32) -> bool
    {
        match &self.cull
        {
            ShadowCull::Sphere { center: c, radius: r } =>
            {
                (center - c).norm_squared() <= (r + radius) * (r + radius)
            },
            ShadowCull::Beam { center: c, radius: r, dir } =>
            {
                // distance of the object center to the cascade axis (line through c along dir)
                let to_center = center - c;
                let along = to_center.dot(dir);
                let perpendicular = to_center - dir * along;

                perpendicular.norm_squared() <= (r + radius) * (r + radius)
            },
            ShadowCull::None => true,
        }
    }
}

pub fn shadow_view_count(light: &Light) -> u32
{
    if !light.enabled || !light.cast_shadow
    {
        return 0;
    }

    match light.light_type
    {
        LightType::Directional | LightType::Sun => SHADOW_CASCADES,
        LightType::Spot => 1,
        LightType::Point => 6,
        LightType::Hemispheric => 0,
    }
}

// per light: (first atlas layer or -1, amount of views)
pub fn assign_shadow_views(lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: usize) -> Vec<(i32, u32)>
{
    let mut assignments = Vec::with_capacity(lights.len());
    let mut next_layer: u32 = 0;

    for (i, light) in lights.iter().enumerate()
    {
        if i >= max_lights
        {
            assignments.push((-1, 0));
            continue;
        }

        let light = light.borrow();
        let light = light.get_ref();

        let views = shadow_view_count(light);

        if views == 0 || next_layer + views > MAX_SHADOW_VIEWS
        {
            assignments.push((-1, 0));
            continue;
        }

        assignments.push((next_layer as i32, views));
        next_layer += views;
    }

    assignments
}

pub fn total_shadow_views(lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: usize) -> u32
{
    assign_shadow_views(lights, max_lights).iter().map(|(_, views)| views).sum()
}

fn spot_view(light: &Light, layer: u32, shadow_max_distance: f32) -> ShadowViewData
{
    // range == 0 means infinite range -> fall back to the global shadow distance
    let far = if light.range > 0.0 { light.range } else { shadow_max_distance.max(0.1) };
    let near = (far * 0.01).max(0.01);

    let dir = light.dir_normalized();
    let up = up_vector_for_direction(&dir);

    // max_angle is the half angle of the spot cone
    let fov = (light.max_angle * 2.0).clamp(0.02, std::f32::consts::PI * 0.98);

    let projection = Perspective3::new(1.0, fov, near, far).to_homogeneous();
    let target = light.pos + dir;
    let view = Isometry3::look_at_rh(&light.pos, &target, &up).to_homogeneous();

    ShadowViewData
    {
        layer,
        view_proj: OPENGL_TO_WGPU_MATRIX * projection * view,
        cull: ShadowCull::Sphere { center: light.pos, radius: far },
    }
}

fn point_views(light: &Light, first_layer: u32, shadow_max_distance: f32, out: &mut Vec<ShadowViewData>)
{
    // range == 0 means infinite range -> fall back to the global shadow distance
    let far = if light.range > 0.0 { light.range } else { shadow_max_distance.max(0.1) };
    let near = (far * 0.01).max(0.01);

    // face order must match the major-axis selection in base.wgsl: +X, -X, +Y, -Y, +Z, -Z
    let faces: [(Vector3<f32>, Vector3<f32>); 6] =
    [
        (Vector3::new( 1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(-1.0, 0.0, 0.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0,  1.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        (Vector3::new(0.0, -1.0, 0.0), Vector3::new(0.0, 0.0, 1.0)),
        (Vector3::new(0.0, 0.0,  1.0), Vector3::new(0.0, 1.0, 0.0)),
        (Vector3::new(0.0, 0.0, -1.0), Vector3::new(0.0, 1.0, 0.0)),
    ];

    // slightly more than 90° so PCF taps at face borders stay inside the map
    let fov = std::f32::consts::FRAC_PI_2 * 1.02;
    let projection = Perspective3::new(1.0, fov, near, far).to_homogeneous();

    for (face, (dir, up)) in faces.iter().enumerate()
    {
        let target = light.pos + dir;
        let view = Isometry3::look_at_rh(&light.pos, &target, up).to_homogeneous();

        out.push(ShadowViewData
        {
            layer: first_layer + face as u32,
            view_proj: OPENGL_TO_WGPU_MATRIX * projection * view,
            cull: ShadowCull::Sphere { center: light.pos, radius: far },
        });
    }
}

fn directional_views(light: &Light, cam_data: Option<&CameraData>, first_layer: u32, shadow_map_size: u32, shadow_max_distance: f32, out: &mut Vec<ShadowViewData>)
{
    let dir = light.dir_normalized();

    let cam_data = match cam_data
    {
        Some(cam_data) => cam_data,
        None =>
        {
            // no active camera to fit the cascades to -> degenerate views (shader treats them as "not shadowed")
            for cascade in 0..SHADOW_CASCADES
            {
                out.push(ShadowViewData
                {
                    layer: first_layer + cascade,
                    view_proj: Matrix4::zeros(),
                    cull: ShadowCull::None,
                });
            }
            return;
        }
    };

    let near = cam_data.clipping_near.max(0.001);
    let far_full = cam_data.clipping_far.max(near + 0.01);
    let far = far_full.min(shadow_max_distance).max(near + 0.01);

    // camera frustum corners in world space (GL NDC: z = -1 near, z = +1 far)
    let inverse_view_proj = cam_data.view_inverse * cam_data.projection_inverse;

    let ndc_corners = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
    let mut near_corners = [Point3::origin(); 4];
    let mut far_corners = [Point3::origin(); 4];

    for (i, (x, y)) in ndc_corners.iter().enumerate()
    {
        let near_h = inverse_view_proj * Vector4::new(*x, *y, -1.0, 1.0);
        let far_h = inverse_view_proj * Vector4::new(*x, *y, 1.0, 1.0);

        near_corners[i] = Point3::from(near_h.xyz() / near_h.w);
        far_corners[i] = Point3::from(far_h.xyz() / far_h.w);
    }

    // practical split scheme: blend between logarithmic and uniform splits
    let mut splits = vec![near];
    for i in 1..=SHADOW_CASCADES
    {
        let p = i as f32 / SHADOW_CASCADES as f32;
        let log_split = near * (far / near).powf(p);
        let uniform_split = near + (far - near) * p;

        splits.push(SHADOW_CSM_SPLIT_LAMBDA * log_split + (1.0 - SHADOW_CSM_SPLIT_LAMBDA) * uniform_split);
    }

    let up = up_vector_for_direction(&dir);

    for cascade in 0..SHADOW_CASCADES as usize
    {
        // frustum slice corners (lerp along the frustum edge rays)
        let t0 = (splits[cascade] - near) / (far_full - near);
        let t1 = (splits[cascade + 1] - near) / (far_full - near);

        let mut corners = [Point3::origin(); 8];
        for i in 0..4
        {
            let edge = far_corners[i] - near_corners[i];
            corners[i] = near_corners[i] + edge * t0;
            corners[i + 4] = near_corners[i] + edge * t1;
        }

        // enclosing sphere (stable fit: independent of camera rotation)
        let mut center = Vector3::zeros();
        for corner in &corners
        {
            center += corner.coords;
        }
        center /= 8.0;

        let center = Point3::from(center);
        let mut radius: f32 = 0.0;
        for corner in &corners
        {
            radius = radius.max((corner - center).norm());
        }
        radius = radius.max(0.01);

        // light view + ortho projection around the sphere, z-range extended backwards for casters behind the slice
        let z_extension = radius * SHADOW_CSM_Z_EXTENSION;
        let eye = center - dir * z_extension;
        let view = Isometry3::look_at_rh(&eye, &center, &up).to_homogeneous();
        let projection = Orthographic3::new(-radius, radius, -radius, radius, 0.0, z_extension + radius * 2.0).to_homogeneous();

        let shadow_matrix = projection * view;

        // texel snapping: shift the projection so the world origin lands on a fixed texel grid.
        // without this, cascades shimmer on camera movement.
        let origin = shadow_matrix * Vector4::new(0.0, 0.0, 0.0, 1.0);
        let half_size = shadow_map_size.max(MIN_SHADOW_MAP_SIZE) as f32 / 2.0;

        let snapped_x = ((origin.x * half_size).round() - origin.x * half_size) / half_size;
        let snapped_y = ((origin.y * half_size).round() - origin.y * half_size) / half_size;
        let snap = Matrix4::new_translation(&Vector3::new(snapped_x, snapped_y, 0.0));

        out.push(ShadowViewData
        {
            layer: first_layer + cascade as u32,
            view_proj: OPENGL_TO_WGPU_MATRIX * snap * shadow_matrix,

            // the square shadow map circumscribes the fitted sphere: its corner regions reach
            // sqrt(2) * radius from the cascade axis - casters there must not be culled,
            // otherwise shadows vanish for receivers sampling those corners
            cull: ShadowCull::Beam { center, radius: radius * std::f32::consts::SQRT_2, dir },
        });
    }
}

pub fn compute_shadow_views(lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: usize, cam_data: Option<&CameraData>, shadow_map_size: u32, shadow_max_distance: f32) -> Vec<ShadowViewData>
{
    let assignments = assign_shadow_views(lights, max_lights);

    let mut views = vec![];

    for (i, light) in lights.iter().enumerate()
    {
        let (first_layer, view_count) = assignments[i];
        if first_layer < 0 || view_count == 0
        {
            continue;
        }

        let first_layer = first_layer as u32;

        let light = light.borrow();
        let light = light.get_ref();

        match light.light_type
        {
            LightType::Directional | LightType::Sun => directional_views(light, cam_data, first_layer, shadow_map_size, shadow_max_distance, &mut views),
            LightType::Spot => views.push(spot_view(light, first_layer, shadow_max_distance)),
            LightType::Point => point_views(light, first_layer, shadow_max_distance, &mut views),
            LightType::Hemispheric => {},
        }
    }

    views
}

// ******************** ShadowBuffer ********************

pub struct ShadowBuffer
{
    layers: u32,
    size: u32,

    atlas_texture: wgpu::Texture,
    layer_views: Vec<wgpu::TextureView>,
    atlas_view: wgpu::TextureView,
    comparison_sampler: wgpu::Sampler,

    // per shadow view matrix with 256 byte stride (dynamic offset) - used by the caster pass
    caster_views_buffer: wgpu::Buffer,
    pub caster_bind_group: wgpu::BindGroup,

    // array<mat4x4, MAX_SHADOW_VIEWS> - used by the fragment shader (base.wgsl)
    shadow_views_buffer: wgpu::Buffer,
}

impl RenderItem for ShadowBuffer
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        let atlas = self.layers as u64 * self.size as u64 * self.size as u64 * 4;
        atlas + self.caster_views_buffer.size() + self.shadow_views_buffer.size()
    }
}

impl ShadowBuffer
{
    pub fn caster_bind_layout(wgpu: &mut WGpu) -> wgpu::BindGroupLayout
    {
        wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                wgpu::BindGroupLayoutEntry
                {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer
                    {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some("shadow_view_bind_group_layout"),
        })
    }

    pub fn new(wgpu: &mut WGpu, size: u32) -> ShadowBuffer
    {
        let size = size.max(MIN_SHADOW_MAP_SIZE);

        let caster_views_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("shadow caster views buffer"),
            size: MAX_SHADOW_VIEWS as u64 * SHADOW_VIEW_UNIFORM_STRIDE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shadow_views_buffer = wgpu.device().create_buffer(&wgpu::BufferDescriptor
        {
            label: Some("shadow views buffer"),
            size: (MAX_SHADOW_VIEWS as usize * std::mem::size_of::<ShadowViewUniform>()) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let comparison_sampler = wgpu.device().create_sampler(&wgpu::SamplerDescriptor
        {
            label: Some("shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let caster_bind_layout = Self::caster_bind_layout(wgpu);
        let caster_bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &caster_bind_layout,
            entries:
            &[
                wgpu::BindGroupEntry
                {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding
                    {
                        buffer: &caster_views_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<ShadowViewUniform>() as u64),
                    }),
                }
            ],
            label: Some("shadow_view_bind_group"),
        });

        let (atlas_texture, layer_views, atlas_view) = Self::create_atlas(wgpu, 1, size);

        ShadowBuffer
        {
            layers: 1,
            size,

            atlas_texture,
            layer_views,
            atlas_view,
            comparison_sampler,

            caster_views_buffer,
            caster_bind_group,

            shadow_views_buffer,
        }
    }

    fn create_atlas(wgpu: &mut WGpu, layers: u32, size: u32) -> (wgpu::Texture, Vec<wgpu::TextureView>, wgpu::TextureView)
    {
        let device = wgpu.device();

        let texture = device.create_texture(&wgpu::TextureDescriptor
        {
            label: Some("shadow atlas texture"),
            size: wgpu::Extent3d
            {
                width: size,
                height: size,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::texture::Texture::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[super::texture::Texture::DEPTH_FORMAT],
        });

        let layer_views: Vec<wgpu::TextureView> = (0..layers).map(|layer| texture.create_view(&wgpu::TextureViewDescriptor
        {
            label: Some("shadow atlas layer view"),
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: layer,
            array_layer_count: Some(1),
            ..Default::default()
        })).collect();

        let atlas_view = texture.create_view(&wgpu::TextureViewDescriptor
        {
            label: Some("shadow atlas view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        (texture, layer_views, atlas_view)
    }

    // resizes the atlas to fit all shadow views of the given lights
    // returns true if the atlas was re-created (bind groups referencing it must be re-created)
    pub fn ensure_for_lights(&mut self, wgpu: &mut WGpu, lights: &Vec<RefCell<ChangeTracker<LightItem>>>, max_lights: usize, size: u32) -> bool
    {
        let needed_layers = total_shadow_views(lights, max_lights);
        self.ensure(wgpu, needed_layers, size)
    }

    // returns true if the atlas was re-created (bind groups referencing it must be re-created)
    fn ensure(&mut self, wgpu: &mut WGpu, needed_layers: u32, size: u32) -> bool
    {
        let needed_layers = needed_layers.clamp(1, MAX_SHADOW_VIEWS);
        let size = size.max(MIN_SHADOW_MAP_SIZE);

        if needed_layers == self.layers && size == self.size
        {
            return false;
        }

        let (atlas_texture, layer_views, atlas_view) = Self::create_atlas(wgpu, needed_layers, size);

        self.atlas_texture = atlas_texture;
        self.layer_views = layer_views;
        self.atlas_view = atlas_view;
        self.layers = needed_layers;
        self.size = size;

        true
    }

    pub fn write_views(&self, wgpu: &mut WGpu, views: &Vec<ShadowViewData>)
    {
        let mut fragment_views = [ShadowViewUniform { view_proj: Matrix4::<f32>::zeros().into() }; MAX_SHADOW_VIEWS as usize];

        for view in views
        {
            if view.layer >= MAX_SHADOW_VIEWS
            {
                continue;
            }

            let uniform = ShadowViewUniform { view_proj: view.view_proj.into() };
            fragment_views[view.layer as usize] = uniform;

            wgpu.queue_mut().write_buffer
            (
                &self.caster_views_buffer,
                view.layer as u64 * SHADOW_VIEW_UNIFORM_STRIDE,
                bytemuck::bytes_of(&uniform),
            );
        }

        wgpu.queue_mut().write_buffer(&self.shadow_views_buffer, 0, bytemuck::cast_slice(&fragment_views));
    }

    pub fn layers(&self) -> u32
    {
        self.layers
    }

    pub fn size(&self) -> u32
    {
        self.size
    }

    pub fn get_layer_view(&self, layer: u32) -> &wgpu::TextureView
    {
        &self.layer_views[layer as usize]
    }

    pub fn get_atlas_view(&self) -> &wgpu::TextureView
    {
        &self.atlas_view
    }

    pub fn get_sampler(&self) -> &wgpu::Sampler
    {
        &self.comparison_sampler
    }

    pub fn get_views_buffer(&self) -> &wgpu::Buffer
    {
        &self.shadow_views_buffer
    }
}
