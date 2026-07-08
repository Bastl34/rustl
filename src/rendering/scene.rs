use std::{collections::HashMap, mem::swap, sync::{Arc, RwLock, RwLockReadGuard}, vec};

use nalgebra::{Point3, distance_squared};
use strum::EnumCount;
use strum_macros::EnumCount;
use wgpu::{CommandEncoder, TextureView, RenderPassColorAttachment, BindGroup, util::DeviceExt};

use crate::{component_downcast, component_downcast_mut, console_debug, console_log, console_warning, helper::image::float32_to_grayscale, render_item_impl_default, rendering::{bind_groups::{depth_export::DepthExportBindGroup, hzb_downsample::HZBDownsampleBindGroup, hzb_occlusion_check::HZBOcclusionCheckBindGroup, ssao::{SsaoBindGroup, SsaoUniform}}, bounding_boxes::{BoundingBox, BoundingBoxesBuffer, BOUNDING_BOX_FLAG_OCCLUSION_TEST}, compute_pipeline::ComputePipeline, draw_slots::{DrawSlot, DrawSlotsBuffer, IndirectArgsBuffers, DRAW_INDEXED_ARGS_SIZE}, gpu_timer::{GpuPassTimes, GpuTimer, GpuTimerPass, GpuTimerSegment}, hzb_cull_buffer::HZBCullBuffer, visibility::VisibilityBuffer}, resources::resources, state::{helper::render_item::{RenderItem, get_render_item, get_render_item_mut}, scene::{camera::{Camera, CameraData}, components::{self, alpha::Alpha, component::{Component, ComponentBox}, joint::Joint, material::TextureType, mesh::Mesh, transformation::Transformation}, node::{Node, NodeItem}, scene::SceneData}, state::{State, DEFAULT_XRAY_ALPHA}}};

use super::{wgpu::WGpu, pipeline::Pipeline, texture::Texture, camera::CameraBuffer, instance::InstanceBuffer, vertex_buffer::VertexBuffer, light::LightBuffer, shadow::{self, ShadowBuffer}, bind_groups::{light_cam_scene::LightCamSceneBindGroup, skeleton_morph_target::SkeletonMorphTargetBindGroup}, material::MaterialBuffer, helper::buffer::create_empty_buffer, skeleton::SkeletonBuffer, morph_target::MorphTarget};

type MaterialComponent = crate::state::scene::components::material::Material;

#[derive(Copy, Clone)]
pub struct RenderData<'a>
{
    node: &'a RwLockReadGuard<'a, Box<Node>>,
    material: &'a RwLockReadGuard<'a, ComponentBox>,
    meshes: &'a Vec<RwLockReadGuard<'a, ComponentBox>>,

    has_transparency: bool,
    alpha_index: i64,
    middle: Option<Point3::<f32>>,
    radius: Option<f32>,
}

#[derive(Copy, Clone)]
pub struct UpdateResult
{
    pub scene_changed: bool,
    pub nodes_amount: usize,
    pub bounding_boxes_buffer_recreated: bool,
    pub slots_rebuilt: bool,
    pub slot_buffer_recreated: bool,
    pub instances_updated: bool,
}

impl UpdateResult
{
    pub fn new() -> Self
    {
        Self
        {
            scene_changed: false,
            nodes_amount: 0,
            bounding_boxes_buffer_recreated: false,
            slots_rebuilt: false,
            slot_buffer_recreated: false,
            instances_updated: false,
        }
    }
}

#[derive(Clone)]
pub struct RenderResultForCamera
{
    pub camera_id: u32,
    pub objects_visible: Vec<u32>,
    pub objects_invisible: Vec<u32>,
    pub objects_frustum_culled: u32,
    pub draw_calls: u32,
}

impl RenderResultForCamera
{
    pub fn new() -> Self
    {
        Self
        {
            camera_id: 0,
            objects_visible: Vec::new(),
            objects_invisible: Vec::new(),
            objects_frustum_culled: 0,
            draw_calls: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneUniform
{
    pub gamma: f32,
    pub exposure: f32,
    pub ibl_diffuse_intensity: f32,
    pub xray_alpha: f32,
    pub shadow_max_distance: f32,
    pub ssao_strength: f32, // 0.0 = ssao disabled
}

impl SceneUniform
{
    pub fn new(scene_data: &SceneData, xray_alpha: f32, shadow_max_distance: f32, ssao_strength: f32) -> Self
    {
        let gamma = if let Some(gamma) = scene_data.gamma { gamma } else { 0.0 };
        let exposure = if let Some(exposure) = scene_data.exposure { exposure } else { 0.0 };
        let ibl_diffuse_intensity = if let Some(ibl_diffuse_intensity) = scene_data.ibl_diffuse_intensity { ibl_diffuse_intensity } else { 1.0 };

        Self
        {
            gamma: gamma,
            exposure: exposure,
            ibl_diffuse_intensity: ibl_diffuse_intensity,
            xray_alpha: xray_alpha,
            shadow_max_distance: shadow_max_distance,
            ssao_strength: ssao_strength,
        }
    }
}

#[derive(EnumCount)]
pub enum RenderPipelineType
{
    Depth = 0,
    DepthNoCompare,
    DepthNoWrite,
    DepthNoWriteNoCompare,
    Color,
    ColorNoCompare,
    ColorNoWrite,
    ColorNoWriteNoCompare,

    // hzb,
    DepthExport,

    Shadow,

    Ssao,
    SsaoBlur,
}

#[derive(EnumCount)]
pub enum ComputePipelineType
{
    HzbDownsample = 0,
    HzbOcclusionCheck,
}

pub enum DrawPhase<'a>
{
    Depth { light_cam_bind_group: &'a BindGroup },
    Color { light_cam_bind_group: &'a BindGroup },
    Shadow { shadow_view: &'a shadow::ShadowViewData },
}

pub struct Scene
{
    clear_color: wgpu::Color,

    color_shader: String,
    depth_shader: String,
    shadow_shader: String,

    depth_export_shader: String,
    hzb_downsample_shader: String,
    hzb_occlusion_check_shader: String,

    // one shader module with both fullscreen entry points (fs_main = ssao, fs_blur = blur)
    ssao_shader: String,

    samples: u32,
    pub wireframe_mode: bool,
    pub xray_mode: bool,
    pub xray_alpha: f32,
    pub distance_sorting: bool,
    pub frustum_culling: bool,
    pub occlusion_culling: bool,

    // occlusion culling needs compute shaders + indirect draws (not available on WebGL)
    occlusion_supported: bool,
    occlusion_was_active: bool,

    // the ssao shader does textureLoad on a depth texture, which naga's GLSL backend
    // does not support (WebGL) -> the ssao pipelines are not created there
    ssao_supported: bool,

    update_result: UpdateResult,

    render_pipelines: Vec<Pipeline>,
    compute_pipelines: Vec<ComputePipeline>,

    buffer: wgpu::Buffer,

    pub depth_pass_buffer_texture: Texture,
    pub depth_buffer_texture: Texture,

    // the per camera depth export bind groups sample the depth pass texture
    // -> they have to be recreated when the texture is recreated (resize)
    depth_pass_texture_changed: bool,

    pub shadow: ShadowBuffer,
    pub shadow_enabled: bool,
    pub shadow_max_distance: f32,

    // shadow stats of the last rendered frame
    pub shadow_views: u32,
    pub shadow_draw_calls: u32,

    // ssao render targets (surface sized, 1 sample): raw pass result and blurred result
    // (the blurred texture is sampled by the color pass via the light/cam/scene bind group)
    pub ssao_texture: Texture,
    pub ssao_blur_texture: Texture,

    pub ssao_enabled: bool,
    pub ssao_radius: f32,
    pub ssao_bias: f32,
    pub ssao_strength: f32,

    bounding_boxes_buffer: BoundingBoxesBuffer,

    // one slot per (node, mesh) draw - the slot index is the fixed offset into the indirect
    // args buffers and connects the cpu-recorded draws with the gpu culling results
    draw_slots: DrawSlotsBuffer,

    // hash over the culling-relevant node state without own change tracking
    // (settings, mesh counts, instance counts) - triggers the culling buffer rebuild
    culling_state_hash: u64,

    hzb_cull_buffer: HZBCullBuffer,

    // gpu pass timings via timestamp queries (None if the adapter does not support them)
    gpu_timer: Option<GpuTimer>,

    empty_skeleton: SkeletonBuffer,
    empty_morph_target: MorphTarget,
    empty_skeleton_morph_group: SkeletonMorphTargetBindGroup,
}

impl RenderItem for Scene
{
    render_item_impl_default!();

    fn gpu_usage(&self) -> u64
    {
        self.buffer.size()
        + self.bounding_boxes_buffer.gpu_usage()
        + self.draw_slots.gpu_usage()
        + self.hzb_cull_buffer.gpu_usage()
        + self.shadow.gpu_usage()
        + self.empty_skeleton.gpu_usage()
        + self.empty_morph_target.gpu_usage()
    }
}

impl Scene
{
    pub fn new(wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene, samples: u32) -> Scene
    {
        // shader source
        let color_shader = resources::load_string("shader/base.wgsl").unwrap();
        let depth_shader = resources::load_string("shader/depth.wgsl").unwrap();
        let shadow_shader = resources::load_string("shader/shadow.wgsl").unwrap();
        let depth_export_shader = resources::load_string("shader/depth_export.wgsl").unwrap();
        let hzb_downsample_shader = resources::load_string("shader/compute/hzb_downsample.wgsl").unwrap();
        let hzb_occlusion_check_shader = resources::load_string("shader/compute/occlusion_hzb_check.wgsl").unwrap();
        let ssao_shader = resources::load_string("shader/ssao.wgsl").unwrap();

        let empty_skeleton = SkeletonBuffer::empty(wgpu);
        let empty_morph_target = MorphTarget::empty(wgpu);

        let empty_skeleton_morph_group = SkeletonMorphTargetBindGroup::new(wgpu, "empty", &empty_skeleton, &empty_morph_target);

        let (depth_width, depth_height) = { let config = wgpu.surface_config(); (config.width, config.height) };
        let depth_buffer_texture = Texture::new_depth_texture(wgpu, samples, depth_width, depth_height);
        let depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1, depth_width, depth_height);

        let ssao_texture = Texture::new_ssao_texture(wgpu, "ssao texture", depth_width, depth_height);
        let ssao_blur_texture = Texture::new_ssao_texture(wgpu, "ssao blur texture", depth_width, depth_height);

        let mut render_scene = Self
        {
            clear_color: wgpu::Color::BLACK,

            color_shader,
            depth_shader,
            shadow_shader,
            depth_export_shader,
            hzb_downsample_shader,
            hzb_occlusion_check_shader,
            ssao_shader,

            samples,
            wireframe_mode: false,
            xray_mode: false,
            xray_alpha: DEFAULT_XRAY_ALPHA,
            distance_sorting: true,
            frustum_culling: true,
            occlusion_culling: true,

            occlusion_supported: state.rendering_adapter.occlusion_culling_support,
            occlusion_was_active: false,

            ssao_supported: state.rendering_adapter.ssao_support,

            update_result: UpdateResult::new(),

            render_pipelines: vec![],
            compute_pipelines: vec![],

            buffer: create_empty_buffer(wgpu),

            depth_buffer_texture,
            depth_pass_buffer_texture,

            depth_pass_texture_changed: false,

            shadow: ShadowBuffer::new(wgpu, *state.rendering.shadow_map_resolution.get_ref()),
            shadow_enabled: *state.rendering.shadow.get_ref(),
            shadow_max_distance: state.rendering.shadow_max_distance,

            shadow_views: 0,
            shadow_draw_calls: 0,

            ssao_texture,
            ssao_blur_texture,

            ssao_enabled: state.rendering.ssao,
            ssao_radius: state.rendering.ssao_radius,
            ssao_bias: state.rendering.ssao_bias,
            ssao_strength: state.rendering.ssao_strength,

            bounding_boxes_buffer: BoundingBoxesBuffer::new(wgpu),
            draw_slots: DrawSlotsBuffer::new(wgpu),
            culling_state_hash: 0,

            hzb_cull_buffer: HZBCullBuffer::new(wgpu),

            gpu_timer: GpuTimer::new(wgpu.device()),

            empty_skeleton,
            empty_morph_target,
            empty_skeleton_morph_group
        };

        render_scene.to_buffer(wgpu, scene);

        render_scene.update(wgpu, state, scene);
        render_scene.create_pipelines(wgpu, scene, false);

        render_scene
    }

    pub fn to_buffer(&mut self, wgpu: &mut WGpu, scene: &crate::state::scene::scene::Scene)
    {
        let data = scene.get_data();

        let effective_xray_alpha = if self.xray_mode { self.xray_alpha } else { 1.0 };
        let effective_ssao_strength = if self.ssao_supported && self.ssao_enabled { self.ssao_strength } else { 0.0 };
        let scene_uniform = SceneUniform::new(data, effective_xray_alpha, self.shadow_max_distance, effective_ssao_strength);

        self.buffer = wgpu.device().create_buffer_init
        (
            &wgpu::util::BufferInitDescriptor
            {
                label: Some(&scene.name),
                contents: bytemuck::cast_slice(&[scene_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
        );
    }

    pub fn update_buffer(&mut self, wgpu: &mut WGpu, scene: &crate::state::scene::scene::Scene)
    {
        let data = scene.get_data();

        let effective_xray_alpha = if self.xray_mode { self.xray_alpha } else { 1.0 };
        let effective_ssao_strength = if self.ssao_supported && self.ssao_enabled { self.ssao_strength } else { 0.0 };
        let scene_uniform = SceneUniform::new(data, effective_xray_alpha, self.shadow_max_distance, effective_ssao_strength);

        wgpu.queue_mut().write_buffer(&self.buffer, 0, bytemuck::cast_slice(&[scene_uniform]));
    }

    pub fn get_buffer(&self) -> &wgpu::Buffer
    {
        &self.buffer
    }

    pub fn create_pipelines(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, re_create: bool)
    {
        /*
        Color Bind Group layout:

        - (0) Materials + Textures (node)
        - (1) Lights, Camera, Scene Properties (Tonemapping/HDR/Gamma) (scene)
        - (2) Skeleton (node)
        - (3) Custom (node)
        */

        let light_cam_scene_bind_layout = LightCamSceneBindGroup::bind_layout(wgpu);

        // material and textures
        let mat = scene.get_default_material().unwrap();
        let mat = mat.read().unwrap();
        let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();

        let material_render_item = &mat.get_base().render_item;
        let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
        let material_bind_layout = material_render_item.bind_group_layout.as_ref().unwrap();

        let skeleton_morph_bind_layout = SkeletonMorphTargetBindGroup::bind_layout(wgpu);

        let bind_group_layouts =
        [
            material_bind_layout,
            &light_cam_scene_bind_layout,
            &skeleton_morph_bind_layout
        ];

        // ********** depth pass **********
        // without ssao support the two ssao pipelines at the end of the enum are not created
        let expected_pipelines = if self.ssao_supported { RenderPipelineType::COUNT } else { RenderPipelineType::COUNT - 2 };
        if !re_create || self.render_pipelines.len() < expected_pipelines
        {
            self.render_pipelines.push(Pipeline::new_std(wgpu, "depth pipe all", &self.depth_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, true, true, 1, wgpu::PolygonMode::Fill));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "depth pipe no compare", &self.depth_shader, &bind_group_layouts, scene.get_data().max_lights, true, false, true, true, 1, wgpu::PolygonMode::Fill));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "depth pipe no write", &self.depth_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, false, true, 1, wgpu::PolygonMode::Fill));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "depth pipe no compare no write", &self.depth_shader, &bind_group_layouts, scene.get_data().max_lights, true, false, false, true, 1, wgpu::PolygonMode::Fill));
        }
        else
        {
            self.render_pipelines.get_mut(RenderPipelineType::Depth as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, true, true, true, 1, wgpu::PolygonMode::Fill);
            self.render_pipelines.get_mut(RenderPipelineType::DepthNoCompare as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, false, true, true, 1, wgpu::PolygonMode::Fill);
            self.render_pipelines.get_mut(RenderPipelineType::DepthNoWrite as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, true, false, true, 1, wgpu::PolygonMode::Fill);
            self.render_pipelines.get_mut(RenderPipelineType::DepthNoWriteNoCompare as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, false, false, true, 1, wgpu::PolygonMode::Fill);
        }

        // ********** color pass **********
        let mut additional_textures = vec![];
        additional_textures.push(&self.depth_pass_buffer_texture);

        if !re_create
        {
            let polygon_mode = if self.wireframe_mode { wgpu::PolygonMode::Line } else { wgpu::PolygonMode::Fill };
            self.render_pipelines.push(Pipeline::new_std(wgpu, "color pipe", &self.color_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, true, true, self.samples, polygon_mode));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "color pipe no compare", &self.color_shader, &bind_group_layouts, scene.get_data().max_lights, true, false, true, true, self.samples, polygon_mode));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "color pipe no write", &self.color_shader, &bind_group_layouts, scene.get_data().max_lights, true, true, false, true, self.samples, polygon_mode));
            self.render_pipelines.push(Pipeline::new_std(wgpu, "color pipe no compare no write", &self.color_shader, &bind_group_layouts, scene.get_data().max_lights, true, false, false, true, self.samples, polygon_mode));
        }
        else
        {
            let polygon_mode = if self.wireframe_mode { wgpu::PolygonMode::Line } else { wgpu::PolygonMode::Fill };
            self.render_pipelines.get_mut(RenderPipelineType::Color as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, true, true, true, self.samples, polygon_mode);
            self.render_pipelines.get_mut(RenderPipelineType::ColorNoCompare as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, false, true, true, self.samples, polygon_mode);
            self.render_pipelines.get_mut(RenderPipelineType::ColorNoWrite as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, true, false, true, self.samples, polygon_mode);
            self.render_pipelines.get_mut(RenderPipelineType::ColorNoWriteNoCompare as usize).unwrap().re_create_std(wgpu, &bind_group_layouts, true, false, false, true, self.samples, polygon_mode);
        }

        // ********** depth export pass (for occlusion culling - hzb) **********

        let depth_export_bind_layout = DepthExportBindGroup::bind_layout(wgpu);

        let bind_group_layouts = [ &depth_export_bind_layout ];

        if !re_create
        {
            self.render_pipelines.push(Pipeline::new_depth_export(wgpu, "depth export", &self.depth_export_shader, &bind_group_layouts));
        }
        else
        {
            self.render_pipelines.get_mut(RenderPipelineType::DepthExport as usize).unwrap().re_create_depth_export(wgpu, &bind_group_layouts);
        }

        // ********** shadow pass **********

        let shadow_view_bind_layout = ShadowBuffer::caster_bind_layout(wgpu);

        let bind_group_layouts = [ &shadow_view_bind_layout, &skeleton_morph_bind_layout ];

        if !re_create
        {
            self.render_pipelines.push(Pipeline::new_shadow(wgpu, "shadow", &self.shadow_shader, &bind_group_layouts));
        }
        else
        {
            self.render_pipelines.get_mut(RenderPipelineType::Shadow as usize).unwrap().re_create_shadow(wgpu, &bind_group_layouts);
        }

        // the ssao shader does textureLoad on a depth texture, which naga's GLSL
        // backend does not support -> the ssao pipelines must not even be created there
        // (they are the last enum entries, so all other pipeline indices stay valid)
        if self.ssao_supported
        {
            // ********** ssao pass (depth -> raw occlusion) **********

            let ssao_bind_layout = SsaoBindGroup::ssao_bind_layout(wgpu);

            let bind_group_layouts = [ &ssao_bind_layout ];

            if !re_create
            {
                self.render_pipelines.push(Pipeline::new_fullscreen(wgpu, "ssao", &self.ssao_shader, &bind_group_layouts, Texture::GRAY_FORMAT, "fs_main"));
            }
            else
            {
                self.render_pipelines.get_mut(RenderPipelineType::Ssao as usize).unwrap().re_create_fullscreen(wgpu, &bind_group_layouts, Texture::GRAY_FORMAT, "fs_main");
            }

            // ********** ssao blur pass (raw occlusion -> blurred occlusion) **********

            let ssao_blur_bind_layout = SsaoBindGroup::blur_bind_layout(wgpu);

            let bind_group_layouts = [ &ssao_blur_bind_layout ];

            if !re_create
            {
                self.render_pipelines.push(Pipeline::new_fullscreen(wgpu, "ssao blur", &self.ssao_shader, &bind_group_layouts, Texture::GRAY_FORMAT, "fs_blur"));
            }
            else
            {
                self.render_pipelines.get_mut(RenderPipelineType::SsaoBlur as usize).unwrap().re_create_fullscreen(wgpu, &bind_group_layouts, Texture::GRAY_FORMAT, "fs_blur");
            }
        }

        // compute shaders are not available on all adapters (WebGL) -> the occlusion
        // culling pipelines must not even be created there
        if self.occlusion_supported
        {
            // ********** downsample pass (for occlusion culling - hzb) **********

            let hzb_downsample_bind_layout = HZBDownsampleBindGroup::bind_layout(wgpu);

            let bind_group_layouts = [ &hzb_downsample_bind_layout ];

            if !re_create
            {
                self.compute_pipelines.push(ComputePipeline::new_hzb_downsample_compute(wgpu, "hzb downsample", &self.hzb_downsample_shader, &bind_group_layouts));
            }
            else
            {
                self.compute_pipelines.get_mut(ComputePipelineType::HzbDownsample as usize).unwrap().re_create_hzb_downsample_compute(wgpu, &bind_group_layouts);
            }

            // ********** occlusion check pass (for occlusion culling - hzb) **********

            let hzb_occlusion_check_bind_layout = HZBOcclusionCheckBindGroup::bind_layout(wgpu);

            let bind_group_layouts = [ &hzb_occlusion_check_bind_layout ];

            if !re_create
            {
                self.compute_pipelines.push(ComputePipeline::new_hzb_occlusion_check_compute(wgpu, "hzb occlusion check", &self.hzb_occlusion_check_shader, &bind_group_layouts));
            }
            else
            {
                self.compute_pipelines.get_mut(ComputePipelineType::HzbOcclusionCheck as usize).unwrap().re_create_hzb_occlusion_check_compute(wgpu, &bind_group_layouts);
            }
        }
    }

    pub fn update_textures(&mut self, _wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene)
    {
        // check if the scene env texture has changed
        if let Some(env_tex) = &scene.get_data().environment_texture
        {
            let enabled = env_tex.enabled;
            if let Some(env_tex) = env_tex.get()
            {
                if enabled && env_tex.read().unwrap().get_data_tracker().changed()
                {
                    console_log!("update all materials");
                    let env_texture_id = env_tex.read().unwrap().id;

                    for (_, material) in &mut scene.materials
                    {
                        let mut material = material.write().unwrap();
                        let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

                        if !material.has_texture(TextureType::Environment) || material.has_texture_id(env_texture_id)
                        {
                            material.get_data_mut().force_change();
                        }
                    }
                }
            }
        }
    }

    pub fn update_materials(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene)
    {
        let default_env_map = scene.get_data().environment_texture.clone();

        for (_material_id, material) in &mut scene.materials
        {
            let mut material = material.write().unwrap();
            let material = material.as_any_mut().downcast_mut::<MaterialComponent>().unwrap();

            let material_changed = material.get_data_mut().consume_change();

            if material_changed || material.get_base().render_item.is_none()
            {
                console_log!("material render item recreate");
                let render_item: MaterialBuffer = MaterialBuffer::new(wgpu, &material, default_env_map.clone(), None);
                material.get_base_mut().render_item = Some(Box::new(render_item));
            }
            else if material_changed || self.update_result.scene_changed
            {
                let mut render_item = material.get_base_mut().render_item.take();

                {
                    let render_item = get_render_item_mut::<MaterialBuffer>(render_item.as_mut().unwrap());
                    render_item.to_buffers(wgpu, material, default_env_map.clone(), None);
                    render_item.create_binding_groups(wgpu, material, default_env_map.clone(), None);
                }

                material.get_base_mut().render_item = render_item;

                console_log!("material render item update");
            }
        }
    }

    pub fn update_light_cameras_shadows(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, shadow_map_size: u32, shadow_enabled_changed: bool, ssao_params_changed: bool)
    {
        // ********** lights: all **********
        let max_lights = scene.get_data().max_lights;
        let (lights, all_lights_changed) = scene.lights.consume_borrow();
        if all_lights_changed || self.update_result.scene_changed || shadow_enabled_changed
        {
            if scene.lights_render_item.is_none()
            {
                let lights_buffer = LightBuffer::new(wgpu, format!("{} lights buffer", scene.name).to_string(), lights, max_lights, self.shadow_enabled);
                scene.lights_render_item = Some(Box::new(lights_buffer));
            }

            let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
            render_item.to_buffer(wgpu, lights, self.shadow_enabled);

            //console_log!(" ============ lights updated");
        }

        // ********** light: check each **********
        if !all_lights_changed
        {
            let mut any_light_changed = false;
            for light in lights.iter()
            {
                let mut light = light.borrow_mut();
                let (_, light_changed) = light.consume_borrow();
                if light_changed
                {
                    any_light_changed = true;
                }
            }

            // a single light change (type/enabled/cast_shadow) can shift the shadow atlas
            // layer assignment of all lights -> re-write the whole buffer
            if any_light_changed
            {
                let render_item = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());
                render_item.to_buffer(wgpu, lights, self.shadow_enabled);

                //console_log!(" ============ lights updated");
            }
        }

        // ********** shadow atlas **********
        let shadow_atlas_recreated = self.shadow.ensure_for_lights(wgpu, lights, max_lights as usize, shadow_map_size, self.shadow_enabled);

        // ********** lights and cameras **********
        for cam in &mut scene.cameras
        {
            let cam_changed = cam.get_data_mut().consume_change();
            let mut hzb_changed = false;
            let mut visibility_changed = false;
            let mut cam_buffer_created = false;

            // create cam render item
            if cam.render_item.is_none()
            {
                cam.update_resolution(wgpu.surface_config().width, wgpu.surface_config().height);
                cam.init_matrices();

                let camera_buffer = CameraBuffer::new(wgpu, &cam);
                cam.render_item = Some(Box::new(camera_buffer));

                cam_buffer_created = true;
            }
            else if cam_changed
            {
                let mut render_item = cam.render_item.take();

                {
                    let render_item = get_render_item_mut::<CameraBuffer>(render_item.as_mut().unwrap());
                    render_item.update_buffer(wgpu, cam.as_ref());
                }

                cam.render_item = render_item;
            }

            // create cam/light/scene bind group
            // (the ssao blur texture is baked into the bind group - it is recreated together
            // with the depth pass texture on resize -> same trigger)
            if cam.bind_group_render_item.is_none() || all_lights_changed || shadow_atlas_recreated || self.depth_pass_texture_changed
            {
                let camera_buffer = get_render_item_mut::<CameraBuffer>(cam.render_item.as_mut().unwrap());
                let lights_buffer = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());

                let light_cam_scene_bind_group = LightCamSceneBindGroup::new(wgpu, &cam.name, &camera_buffer, &lights_buffer, &self);

                cam.bind_group_render_item = Some(Box::new(light_cam_scene_bind_group));
            }

            // ********** ssao bind group + uniform **********
            // no resources at all on adapters without ssao support (WebGL)
            if self.ssao_supported
            {
                // the depth pass and raw ssao texture views are baked into the bind groups
                // -> recreate when those textures were recreated (resize)
                if cam.ssao_bind_group_render_item.is_none() || cam_buffer_created || self.depth_pass_texture_changed
                {
                    let ssao_uniform = SsaoUniform::new(cam.webgpu_projection(), cam.get_data().viewport_px(), self.ssao_radius, self.ssao_bias);
                    let ssao_bind_group = SsaoBindGroup::new(wgpu, &cam.name, &self.depth_pass_buffer_texture, &self.ssao_texture, ssao_uniform);
                    cam.ssao_bind_group_render_item = Some(Box::new(ssao_bind_group));
                }
                else if cam_changed || ssao_params_changed
                {
                    let ssao_uniform = SsaoUniform::new(cam.webgpu_projection(), cam.get_data().viewport_px(), self.ssao_radius, self.ssao_bias);
                    let ssao_bind_group = get_render_item::<SsaoBindGroup>(cam.ssao_bind_group_render_item.as_ref().unwrap());
                    ssao_bind_group.update_uniform(wgpu, ssao_uniform);
                }
            }

            // ********** occlusion culling resources **********
            // not created at all on adapters without compute/indirect support (WebGL)
            if !self.occlusion_supported
            {
                continue;
            }

            // create/recreate hzb texture
            if cam_buffer_created || cam.hzb_texture_render_item.is_none() || cam.get_viewport_width_in_px() != get_render_item::<Texture>(cam.hzb_texture_render_item.as_ref().unwrap()).width || cam.get_viewport_height_in_px() != get_render_item::<Texture>(cam.hzb_texture_render_item.as_ref().unwrap()).height
            {
                let hzb_texture = Texture::new_hzb_texture(wgpu, cam.get_viewport_width_in_px(), cam.get_viewport_height_in_px());
                let hzb_downsample_bind_group = HZBDownsampleBindGroup::new(wgpu, "hzb downsample", &hzb_texture);

                cam.hzb_texture_render_item = Some(Box::new(hzb_texture));
                cam.hzb_downsample_bind_group_render_item = Some(Box::new(hzb_downsample_bind_group));

                hzb_changed = true;
            }

            // create/recreate visibility buffer
            if cam.visibility_buffer_render_item.is_none()
            {
                let visibility_buffer = VisibilityBuffer::new(wgpu, self.update_result.nodes_amount);
                cam.visibility_buffer_render_item = Some(Box::new(visibility_buffer));

                visibility_changed = true;
            }
            // re-create buffer if needed (when nodes amount increased)
            else
            {
                let render_item = get_render_item::<VisibilityBuffer>(cam.visibility_buffer_render_item.as_mut().unwrap());
                if render_item.buffer_size < self.update_result.nodes_amount
                {
                    let visibility_buffer = VisibilityBuffer::new(wgpu, self.update_result.nodes_amount);
                    cam.visibility_buffer_render_item = Some(Box::new(visibility_buffer));
                    visibility_changed = true;

                    console_log!("Re-created visibility buffer for camera {}", cam.name);
                }
            }

            // reset the previous visibility when the node order changed (slot/node indices are no longer valid)
            if self.update_result.slots_rebuilt && !visibility_changed
            {
                let visibility_buffer = get_render_item::<VisibilityBuffer>(cam.visibility_buffer_render_item.as_ref().unwrap());
                visibility_buffer.reset_all_visible(wgpu);
            }

            // create or re-create the per camera indirect args buffers
            let mut indirect_args_changed = false;
            if cam.indirect_args_render_item.is_none() || get_render_item::<IndirectArgsBuffers>(cam.indirect_args_render_item.as_ref().unwrap()).buffer_size < self.draw_slots.slots.len()
            {
                let indirect_args = IndirectArgsBuffers::new(wgpu, &self.draw_slots.slots);
                cam.indirect_args_render_item = Some(Box::new(indirect_args));

                indirect_args_changed = true;
            }
            else if self.update_result.slots_rebuilt
            {
                // slots changed -> reset to "everything visible" until the next occlusion check results exist
                let indirect_args = get_render_item::<IndirectArgsBuffers>(cam.indirect_args_render_item.as_ref().unwrap());
                indirect_args.reset_full_visible(wgpu, &self.draw_slots.slots);
            }

            // create or re-create the depth export bind group (depth -> hzb, remapped to the camera viewport)
            {
                let viewport = cam.get_data().get_viewport();

                // uv rect of the camera viewport inside the depth texture (v=0 is the top row)
                let viewport_offset_scale =
                [
                    viewport.x,
                    1.0 - viewport.y - viewport.height,
                    viewport.width,
                    viewport.height,
                ];

                if cam.depth_export_bind_group_render_item.is_none() || cam_buffer_created || self.depth_pass_texture_changed
                {
                    let depth_export_bind_group = DepthExportBindGroup::new(wgpu, &cam.name, &self.depth_pass_buffer_texture, viewport_offset_scale);
                    cam.depth_export_bind_group_render_item = Some(Box::new(depth_export_bind_group));
                }
                else if cam_changed
                {
                    // only the viewport rect can change here - update the uniform in place
                    // instead of recreating buffer/sampler/bind group every frame while the camera moves
                    let depth_export_bind_group = get_render_item::<DepthExportBindGroup>(cam.depth_export_bind_group_render_item.as_ref().unwrap());
                    depth_export_bind_group.update_viewport(wgpu, viewport_offset_scale);
                }
            }

            // create or re-create occlusion bind group
            if cam.hzb_occlusion_bind_group_render_item.is_none() || hzb_changed || visibility_changed || cam_buffer_created || indirect_args_changed || self.update_result.bounding_boxes_buffer_recreated || self.update_result.slot_buffer_recreated
            {
                console_debug!("create/re-create occlusion bind group for cam {}", cam.name);

                let cam_buffer = &get_render_item::<CameraBuffer>(cam.render_item.as_ref().unwrap());
                let visibility_buffer = &get_render_item::<VisibilityBuffer>(cam.visibility_buffer_render_item.as_ref().unwrap());
                let hzb_texture = &get_render_item::<Texture>(cam.hzb_texture_render_item.as_ref().unwrap());
                let indirect_args = &get_render_item::<IndirectArgsBuffers>(cam.indirect_args_render_item.as_ref().unwrap());

                let hzb_occlusion_bind_group = HZBOcclusionCheckBindGroup::new(wgpu, "occlusion", cam_buffer, visibility_buffer, &self.bounding_boxes_buffer, &self.hzb_cull_buffer, hzb_texture, &self.draw_slots, indirect_args);
                cam.hzb_occlusion_bind_group_render_item = Some(Box::new(hzb_occlusion_bind_group));
            }
        }

        self.depth_pass_texture_changed = false;
    }

    pub fn update_nodes(&mut self, wgpu: &mut WGpu, nodes: &mut Vec<Arc<RwLock<Box<Node>>>>)
    {
        let mut instance_buffers_updated = false;
        let mut vertex_buffers_updated = false;

        // go in reverse to find parent transformations for child nodes
        for node_id in (0..nodes.len()).rev()
        {
            let mut create_new_skeleton_morph_target_bind_group = false;

            // ********** vertex buffer and morph target/s **********
            {
                let node_arc = nodes.get(node_id).unwrap();

                let node = node_arc.read().unwrap();
                let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();

                if let Some(mesh) = mesh
                {
                    component_downcast_mut!(mesh, crate::state::scene::components::mesh::Mesh);

                    let mut mesh_data_changed = mesh.get_data_mut().consume_change();

                    // mesh resource
                    if let Some(mesh_resource) = mesh.mesh_resource.as_mut()
                    {
                        let mut mesh_resource = mesh_resource.write().unwrap();
                        let mesh_resource_data_changed = mesh_resource.get_data_mut().consume_change();
                        if mesh_resource.render_item.is_none() || mesh_resource_data_changed
                        {
                            let vertex_buffer = VertexBuffer::new(wgpu, "vertex buffer", mesh_resource.get_data());
                            mesh_resource.render_item = Some(Box::new(vertex_buffer));

                            mesh_data_changed = true;
                            vertex_buffers_updated = true; // index counts are baked into the draw slots
                        }

                        // morph target
                        if mesh_data_changed
                        {
                            //let vertex_buffer = VertexBuffer::new(wgpu, "vertex buffer", mesh.get_data());
                            //mesh.get_base_mut().render_item = Some(Box::new(vertex_buffer));

                            if MorphTarget::get_morph_targets(mesh_resource.get_data()) > 0
                            {
                                let morph_target = MorphTarget::new(wgpu, "morph target", mesh_resource.get_data());
                                mesh.morph_target_render_item = Some(Box::new(morph_target));

                                create_new_skeleton_morph_target_bind_group = true;
                            }
                        }
                    }
                }
            }

            // ********** morph target/s **********
            {
                let node_arc = nodes.get(node_id).unwrap();

                let has_changed_morph_target_weights = Self::consume_changed_morph_targets(node_arc.clone());

                if has_changed_morph_target_weights
                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let node = node.read().unwrap();

                    let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();
                    if let Some(mesh) = mesh
                    {
                        let weights = node.get_morph_target_weights_vec();

                        if let Some(weights) = weights
                        {
                            component_downcast_mut!(mesh, crate::state::scene::components::mesh::Mesh);
                            let morph_render_item = get_render_item_mut::<MorphTarget>(mesh.morph_target_render_item.as_mut().unwrap());

                            morph_render_item.update_buffer(wgpu, &weights);
                        }
                    }
                }
            }

            // ********** skeleton **********
            {
                let node_arc = nodes.get_mut(node_id).unwrap();

                if node_arc.read().unwrap().skin.len() > 0
                {
                    if node_arc.read().unwrap().skeleton_render_item.is_none()
                    {
                        let mut node_write = node_arc.write().unwrap();

                        let joint_matrices = node_write.get_joint_transform_vec(true);
                        if let Some(joint_matrices) = joint_matrices
                        {
                            let skeleton_buffer = SkeletonBuffer::new(wgpu, "skeleton", &joint_matrices);
                            node_write.skeleton_render_item = Some(Box::new(skeleton_buffer));
                            create_new_skeleton_morph_target_bind_group = true;
                        }
                        else
                        {
                            let skeleton_buffer = SkeletonBuffer::new(wgpu, "skeleton", &vec![]);
                            node_write.skeleton_render_item = Some(Box::new(skeleton_buffer));
                            create_new_skeleton_morph_target_bind_group = true;
                        }
                    }
                    else if Self::has_changed_joints(node_arc.clone())
                    {
                        let joint_matrices = node_arc.read().unwrap().get_joint_transform_vec(true);
                        if let Some(joint_matrices) = joint_matrices
                        {
                            let mut node_write = node_arc.write().unwrap();
                            let render_item = get_render_item_mut::<SkeletonBuffer>(node_write.skeleton_render_item.as_mut().unwrap());
                            render_item.update_buffer(wgpu, &joint_matrices);
                        }
                    }
                }
            }

            // ********** skeleton and morph target/s bind group **********
            {
                let node = nodes.get_mut(node_id).unwrap();
                let mut node = node.write().unwrap();

                let mesh = node.find_component::<crate::state::scene::components::mesh::Mesh>();

                if let Some(mesh) = mesh
                {
                    component_downcast!(mesh, crate::state::scene::components::mesh::Mesh);

                    if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                    {
                        let mesh_resource = mesh_resource.read().unwrap();

                        let has_morph_targets = MorphTarget::get_morph_targets(mesh_resource.get_data()) > 0;
                        let has_skeleton = node.skin.len() > 0;

                        if has_morph_targets || has_skeleton
                        {
                            if node.skeleton_morph_target_bind_group_render_item.is_none() || create_new_skeleton_morph_target_bind_group
                            {
                                // skeleton and morph targets
                                if has_morph_targets && has_skeleton
                                {
                                    let skeleton_render_item = get_render_item::<SkeletonBuffer>(node.skeleton_render_item.as_ref().unwrap());
                                    let morph_render_item = get_render_item::<MorphTarget>(mesh.morph_target_render_item.as_ref().unwrap());

                                    let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Skeleton Morph Target", &skeleton_render_item, &morph_render_item);
                                    node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                                }
                                // only skeleton
                                else if has_skeleton
                                {
                                    let skeleton_render_item = get_render_item::<SkeletonBuffer>(node.skeleton_render_item.as_ref().unwrap());

                                    let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Skeleton and Empty Morph Target", &skeleton_render_item, &self.empty_morph_target);
                                    node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                                }
                                // only morph targets
                                else if has_morph_targets
                                {
                                    let morph_render_item = get_render_item::<MorphTarget>(mesh.morph_target_render_item.as_ref().unwrap());

                                    let skeleton_morph_target_bind_group_render_item = SkeletonMorphTargetBindGroup::new(wgpu, "Empty Skeleton Morph Target", &self.empty_skeleton, &morph_render_item);
                                    node.skeleton_morph_target_bind_group_render_item = Some(Box::new(skeleton_morph_target_bind_group_render_item));
                                }
                            }
                        }
                    }
                }
            }

            // ********** instances all **********
            let mut all_instances_changed;
            {
                let node_arc = nodes.get_mut(node_id).unwrap();

                {
                    let mut node_write = node_arc.write().unwrap();
                    all_instances_changed = node_write.instances.consume_change();
                }

                {
                    let node = node_arc.write().unwrap();
                    let trans_component = node.find_component::<Transformation>();
                    if let Some(trans_component) = trans_component
                    {
                        component_downcast_mut!(trans_component, Transformation);
                        all_instances_changed = trans_component.get_data_mut().consume_change() || all_instances_changed;
                    }

                    if !all_instances_changed
                    {
                        let alpha_component = node.find_component::<Alpha>();
                        if let Some(alpha_component) = alpha_component
                        {
                            component_downcast_mut!(alpha_component, Alpha);
                            all_instances_changed = alpha_component.get_data_mut().consume_change() || all_instances_changed;
                        }
                    }
                }

                if all_instances_changed
                {
                    // console_debug!(" ============ instances updated {}", &node.name);
                    instance_buffers_updated = true;

                    let node_instances_count = node_arc.read().unwrap().instances.get_ref().len();

                    let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
                    {
                        let mut node_write = node_arc.write().unwrap();
                        swap(&mut node_write.instance_render_item, &mut render_item);
                    }

                    let can_reuse_buffer = render_item.as_ref()
                        .and_then(|r| r.as_any().downcast_ref::<InstanceBuffer>())
                        .map(|ib| ib.count as usize == node_instances_count)
                        .unwrap_or(false);

                    if can_reuse_buffer
                    {
                        let node = node_arc.read().unwrap();
                        let instances = node.instances.get_ref();
                        let instance_buffer = get_render_item_mut::<InstanceBuffer>(render_item.as_mut().unwrap());
                        instance_buffer.write_all_to_buffer(wgpu, instances);
                    }
                    else
                    {
                        let node = node_arc.read().unwrap();
                        let instances = node.instances.get_ref();
                        render_item = Some(Box::new(InstanceBuffer::new(wgpu, "instance buffer", instances)));
                    }

                    node_arc.write().unwrap().instance_render_item = render_item;
                }
            }

            // ********** mark instances as updated **********
            if all_instances_changed
            {
                {
                    let node = nodes.get(node_id).unwrap();
                    let node = node.read().unwrap();
                    let instances = node.instances.get_ref();

                    for instance in instances
                    {
                        let mut instance = instance.write().unwrap();
                        instance.get_data_mut().consume_change();
                    }
                }
            }

            // ********** instances check each **********
            if !all_instances_changed
            {
                let mut render_item: Option<Box<dyn RenderItem + Send + Sync>> = None;
                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let mut node = node.write().unwrap();

                    swap(&mut node.instance_render_item, &mut render_item);
                }

                {
                    let node = nodes.get(node_id).unwrap();
                    let node = node.read().unwrap();
                    let instances_ref = node.instances.get_ref();

                    for (i, instance) in instances_ref.iter().enumerate()
                    {
                        let mut instance = instance.write().unwrap();
                        //let (instance_data, mut instance_changed) = instance.get_data_mut().consume_borrow();
                        let instance_changed = instance.get_data_mut().consume_change();

                        //instance_changed = Self::find_changed_instance_data(instance) || instance_changed;

                        if instance_changed
                        {
                            let render_item = get_render_item_mut::<InstanceBuffer>(render_item.as_mut().unwrap());
                            render_item.update_buffer(wgpu, &instance, i);

                            // console_debug!(" ============ ONE instance updated {}", &node.name);

                            instance_buffers_updated = true;
                        }
                    }
                }

                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let mut node = node.write().unwrap();

                    swap(&mut render_item, &mut node.instance_render_item);
                }
            }
        }

        // ********** bounding box / draw slot buffers (occlusion culling) **********
        if self.occlusion_supported
        {
            // cheap per-frame hash over everything that is baked into the culling buffers but
            // has no own change tracker: node ids/count, culling-relevant settings, mesh counts
            // and instance counts. detects mesh component add/remove and programmatic settings
            // changes (e.g. gizmo code setting depth_test directly)
            fn fnv(hash: u64, value: u64) -> u64 { (hash ^ value).wrapping_mul(0x100000001b3) }

            let mut culling_hash: u64 = 0xcbf29ce484222325;
            for node in nodes.iter()
            {
                let node = node.read().unwrap();
                let settings = &node.settings;

                culling_hash = fnv(culling_hash, node.id as u64);
                culling_hash = fnv(culling_hash,
                    ((settings.occlusion_culling as u64) << 3)
                    | ((settings.depth_test as u64) << 2)
                    | ((settings.depth_write as u64) << 1)
                    | (settings.visible as u64));
                culling_hash = fnv(culling_hash, node.get_meshes_with_mesh_resource().len() as u64);
                culling_hash = fnv(culling_hash, node.instances.get_ref().len() as u64);
            }

            let culling_state_changed = culling_hash != self.culling_state_hash;
            self.culling_state_hash = culling_hash;

            // rebuild when transforms/geometry changed or the culling-relevant state hash moved
            if instance_buffers_updated || vertex_buffers_updated || culling_state_changed
            {
                let mut buffer_data: Vec<BoundingBox> = Vec::with_capacity(nodes.len());
                let mut slots: Vec<DrawSlot> = vec![];
                let mut slot_map: HashMap<u32, (u32, u32)> = HashMap::new();

                for node_id in 0..nodes.len()
                {
                    let node = nodes.get_mut(node_id).unwrap();
                    let node = node.read().unwrap();

                    // TODO: optimize - only update if node or instances changed -> case base on node_id
                    let bbox_for_all_instances =
                    {
                        node.get_bounding_box_for_all_instances_from_cached_transform()
                    };

                    // one draw slot per mesh - the slot index is the offset into the indirect args buffers
                    let node_meshes = node.get_meshes_with_mesh_resource();
                    let slot_start = slots.len() as u32;
                    let slot_count = node_meshes.len() as u32;

                    if slot_count > 0
                    {
                        slot_map.insert(node.id, (slot_start, slot_count));

                        let instance_count = node.instances.get_ref().len() as u32;

                        for mesh in &node_meshes
                        {
                            let mesh = mesh.read().unwrap();
                            let mesh = mesh.as_any().downcast_ref::<Mesh>().unwrap();

                            let mut index_count = 0;
                            if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                            {
                                let mesh_resource = mesh_resource.read().unwrap();
                                if let Some(render_item) = mesh_resource.render_item.as_ref()
                                {
                                    index_count = get_render_item::<VertexBuffer>(render_item).get_index_count();
                                }
                            }

                            slots.push(DrawSlot { node_index: node_id as u32, index_count, instance_count, _padding: 0 });
                        }
                    }

                    // objects without depth test cannot be tested against the hzb (they are drawn
                    // on top of everything) and hidden objects are never drawn at all - both are
                    // reported as "visible" so the culling stats stay meaningful
                    let mut flags = 0;
                    if node.settings.occlusion_culling && node.settings.depth_test && node.settings.visible
                    {
                        flags |= BOUNDING_BOX_FLAG_OCCLUSION_TEST;
                    }

                    if let Some((min, max)) = bbox_for_all_instances
                    {
                        buffer_data.push(BoundingBox::new(node.id, &min, &max, flags, slot_start, slot_count));
                    }
                    else
                    {
                        buffer_data.push(BoundingBox::new(node.id, &Point3::origin(), &Point3::origin(), 0, slot_start, slot_count));
                    }
                }

                self.update_result.bounding_boxes_buffer_recreated = self.bounding_boxes_buffer.update(wgpu, &buffer_data);

                // the bounding boxes change on every transform update, but the slot table only
                // changes on topology changes (nodes/meshes/instance counts). only a real slot
                // change may set slots_rebuilt - it resets the per camera visibility state and
                // would otherwise disable the occlusion culling in scenes with animated objects
                let slots_changed = slots != self.draw_slots.slots || slot_map != self.draw_slots.slot_map;
                if slots_changed
                {
                    self.update_result.slot_buffer_recreated = self.draw_slots.update(wgpu, slots, slot_map);
                    self.update_result.slots_rebuilt = true;
                    console_debug!("draw slots updated");
                }
            }

            // ********** occlusion culling param buffer **********
            if self.hzb_cull_buffer.num_objects != nodes.len() || self.hzb_cull_buffer.num_slots != self.draw_slots.slots.len()
            {
                self.hzb_cull_buffer.update(wgpu, nodes.len() as u32, self.draw_slots.slots.len() as u32);

                console_debug!("occlusion culling param buffer updated");
            }
        }

        self.update_result.instances_updated = instance_buffers_updated;
    }

    pub fn consume_changed_morph_targets(node: Arc<RwLock<Box<Node>>>) -> bool
    {
        let node = node.read().unwrap();
        let morph_target_components = node.find_components::<components::morph_target::MorphTarget>();

        let mut has_changed = false;
        for morph_target in morph_target_components
        {
            component_downcast_mut!(morph_target, components::morph_target::MorphTarget);
            has_changed = morph_target.get_data_mut().consume_change() || has_changed;
        }

        has_changed
    }

    pub fn has_changed_joints(mesh_node: Arc<RwLock<Box<Node>>>) -> bool
    {
        let node = mesh_node.read().unwrap();

        for joint in &node.skin
        {
            if let Some(joint) = joint.as_ref()
            {
                let joint = joint.read().unwrap();
                let joint_component = joint.find_component::<Joint>();
                if let Some(joint_component) = joint_component
                {
                    component_downcast!(joint_component, Joint);

                    if joint_component.get_data_tracker().changed()
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn consume_changed_joints(nodes: &Vec<Arc<RwLock<Box<Node>>>>)
    {
        for node in nodes
        {
            let node = node.read().unwrap();
            let joint_component = node.find_component::<Joint>();

            if joint_component.is_none()
            {
                continue;
            }

            if let Some(joint_component) = joint_component
            {
                component_downcast_mut!(joint_component, Joint);

                joint_component.get_data_mut().consume_change();
            }
        }
    }

    pub fn update(&mut self, wgpu: &mut WGpu, state: &mut State, scene: &mut crate::state::scene::scene::Scene)
    {
        // ********** clear color **********
        let (clear_color, clear_color_changed) = state.rendering.clear_color.consume_borrow();

        if clear_color_changed
        {
            self.clear_color = wgpu::Color
            {
                r: clear_color.x as f64,
                g: clear_color.y as f64,
                b: clear_color.z as f64,
                a: 1.0,
            };
        }

        self.update_result = UpdateResult::new();

        // ********** dynamic items **********
        self.update_textures(wgpu, scene);

        let mut all_nodes = Scene::list_all_child_nodes(&scene.nodes, false);
        let scene_changed = scene.get_data_mut().consume_change();
        self.update_result.scene_changed = scene_changed;
        self.update_result.nodes_amount = all_nodes.len();

        self.update_materials(wgpu, scene);

        if scene_changed
        {
            console_log!("scene data changed -> recreate materials/lights/pipelines");

            // update scene buffer
            self.update_buffer(wgpu, scene);

            // update pipelines
            self.create_pipelines(wgpu, scene, true);
        }

        self.update_nodes(wgpu, &mut all_nodes);
        Self::consume_changed_joints(&all_nodes);

        // shadow distance is part of the scene uniform (used for the distance fade in the shader)
        if state.rendering.shadow_max_distance != self.shadow_max_distance
        {
            self.shadow_max_distance = state.rendering.shadow_max_distance;
            self.update_buffer(wgpu, scene);
        }

        // ssao strength/enabled are part of the scene uniform (0.0 = disabled)
        if state.rendering.ssao != self.ssao_enabled || state.rendering.ssao_strength != self.ssao_strength
        {
            self.ssao_enabled = state.rendering.ssao;
            self.ssao_strength = state.rendering.ssao_strength;
            self.update_buffer(wgpu, scene);
        }

        // radius/bias live in the per camera ssao uniforms
        let ssao_params_changed = state.rendering.ssao_radius != self.ssao_radius || state.rendering.ssao_bias != self.ssao_bias;
        if ssao_params_changed
        {
            self.ssao_radius = state.rendering.ssao_radius;
            self.ssao_bias = state.rendering.ssao_bias;
        }

        // toggling shadows changes the atlas layer assignment of all lights -> lights buffer must be re-written
        let shadow_enabled = *state.rendering.shadow.get_ref();
        let shadow_enabled_changed = shadow_enabled != self.shadow_enabled;
        self.shadow_enabled = shadow_enabled;

        self.update_light_cameras_shadows(wgpu, scene, *state.rendering.shadow_map_resolution.get_ref(), shadow_enabled_changed, ssao_params_changed);

        // ********** save image stuff **********
        if state.debug.save_image
        {
            let node_id = 0;
            let node_arc = scene.nodes.get(node_id).unwrap();

            let mat = node_arc.read().unwrap().find_component::<MaterialComponent>();

            if let Some(mat) = mat
            {
                component_downcast!(mat, MaterialComponent);

                let data = mat.get_data();

                if let Some(base_tex) = data.texture_base.as_ref()
                {
                    if let Some(base_tex) = base_tex.get()
                    {
                        let base_tex = base_tex.read().unwrap();
                        let render_item = base_tex.render_item.as_ref().unwrap();
                        let render_item = get_render_item::<Texture>(&render_item);

                        let img_data = render_item.to_image(wgpu, None);
                        img_data.save("data/base_texture.png").unwrap();
                    }
                }

                if let Some(texture_normal) = data.texture_normal.as_ref()
                {
                    if let Some(texture_normal) = texture_normal.get()
                    {
                        let texture_normal = texture_normal.read().unwrap();
                        let render_item = texture_normal.render_item.as_ref().unwrap();
                        let render_item = get_render_item::<Texture>(&render_item);

                        let img_data = render_item.to_image(wgpu, None);
                        img_data.save("data/normal_texture.png").unwrap();
                    }
                }
            }

            state.debug.save_image = false;
        }

        if state.debug.save_depth_pass_image
        {
            let img_data = self.depth_pass_buffer_texture.to_image(wgpu, None);
            img_data.save("data/depth_pass.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_pass_gray.png").unwrap();

            state.debug.save_depth_pass_image = false;
        }

        if state.debug.save_depth_buffer_image
        {
            let img_data = self.depth_buffer_texture.to_image(wgpu, None);
            img_data.save("data/depth_buffer.png").unwrap();

            let img_data_gray = float32_to_grayscale(img_data);
            img_data_gray.save("data/depth_buffer_gray.png").unwrap();

            state.debug.save_depth_buffer_image = false;
        }

        if state.debug.save_hzb_image
        {
            for cam in &scene.cameras
            {
                let hzb_texture = get_render_item::<Texture>(cam.hzb_texture_render_item.as_ref().unwrap());

                let mips = hzb_texture.get_texture().mip_level_count();

                for mip in 0..mips
                {
                    let img_data = hzb_texture.to_image(wgpu, Some(mip));
                    img_data.save(format!("data/hzb_scene{}_cam{}_mip{}.png", scene.id, cam.id, mip)).unwrap();

                    let img_data_gray = float32_to_grayscale(img_data);
                    img_data_gray.save(format!("data/hzb_scene{}_cam{}_mip{}_gray.png", scene.id, cam.id, mip)).unwrap();
                }
            }

            state.debug.save_hzb_image = false;
        }
    }

    pub fn wireframe_mode_update(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, wireframe_mode: bool)
    {
        self.wireframe_mode = wireframe_mode;
        self.create_pipelines(wgpu, scene, true);
    }

    pub fn xray_mode_update(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, xray_mode: bool, xray_alpha: f32)
    {
        self.xray_mode = xray_mode;
        self.xray_alpha = xray_alpha;
        self.update_buffer(wgpu, scene);
    }

    pub fn msaa_sample_size_update(&mut self, wgpu: &mut WGpu, scene: &mut crate::state::scene::scene::Scene, samples: u32)
    {
        self.samples = samples;

        let (depth_width, depth_height) = (self.depth_buffer_texture.width, self.depth_buffer_texture.height);
        self.depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples, depth_width, depth_height);

        //self.update_materials(wgpu, scene, true);
        self.create_pipelines(wgpu, scene, true);
    }

    pub fn resize(&mut self, wgpu: &mut WGpu, scene: &mut Box<crate::state::scene::scene::Scene>, width: u32, height: u32)
    {
        self.depth_buffer_texture = Texture::new_depth_texture(wgpu, self.samples, width, height);
        self.depth_pass_buffer_texture = Texture::new_depth_texture(wgpu, 1, width, height);

        self.ssao_texture = Texture::new_ssao_texture(wgpu, "ssao texture", width, height);
        self.ssao_blur_texture = Texture::new_ssao_texture(wgpu, "ssao blur texture", width, height);

        // the per camera depth export / ssao / light-cam-scene bind groups sample the
        // recreated textures -> recreate them in the next update (cameras added later etc.)
        self.depth_pass_texture_changed = true;

        // rendering keeps running while the update loop is paused (state.pause) -> existing
        // bind groups that bake the recreated texture views are rebuilt right away, otherwise
        // the passes would sample the old (never again written) textures until the next
        // unpaused update
        let scene = scene.as_mut();
        for cam in &mut scene.cameras
        {
            if cam.render_item.is_none()
            {
                continue;
            }

            if cam.bind_group_render_item.is_some() && scene.lights_render_item.is_some()
            {
                let camera_buffer = get_render_item_mut::<CameraBuffer>(cam.render_item.as_mut().unwrap());
                let lights_buffer = get_render_item_mut::<LightBuffer>(scene.lights_render_item.as_mut().unwrap());

                let light_cam_scene_bind_group = LightCamSceneBindGroup::new(wgpu, &cam.name, &camera_buffer, &lights_buffer, &self);
                cam.bind_group_render_item = Some(Box::new(light_cam_scene_bind_group));
            }

            if self.ssao_supported && cam.ssao_bind_group_render_item.is_some()
            {
                let ssao_uniform = SsaoUniform::new(cam.webgpu_projection(), cam.get_data().viewport_px(), self.ssao_radius, self.ssao_bias);
                let ssao_bind_group = SsaoBindGroup::new(wgpu, &cam.name, &self.depth_pass_buffer_texture, &self.ssao_texture, ssao_uniform);
                cam.ssao_bind_group_render_item = Some(Box::new(ssao_bind_group));
            }

            if cam.depth_export_bind_group_render_item.is_some()
            {
                let viewport = cam.get_data().get_viewport();

                // uv rect of the camera viewport inside the depth texture (v=0 is the top row)
                let viewport_offset_scale =
                [
                    viewport.x,
                    1.0 - viewport.y - viewport.height,
                    viewport.width,
                    viewport.height,
                ];

                let depth_export_bind_group = DepthExportBindGroup::new(wgpu, &cam.name, &self.depth_pass_buffer_texture, viewport_offset_scale);
                cam.depth_export_bind_group_render_item = Some(Box::new(depth_export_bind_group));
            }
        }
    }

    pub fn list_all_child_nodes(nodes: &Vec<NodeItem>, check_visibility: bool) -> Vec<NodeItem>
    {
        let mut all_nodes = vec![];

        for node in nodes
        {
            if check_visibility
            {
                let node = node.read().unwrap();
                let visible = node.settings.visible;

                if !visible
                {
                    continue;
                }
            }

            let child_nodes = Scene::list_all_child_nodes(&node.read().unwrap().nodes, check_visibility);

            if node.read().unwrap().settings.render_children_first
            {
                all_nodes.extend(child_nodes);
                all_nodes.push(node.clone());
            }
            else
            {
                all_nodes.push(node.clone());
                all_nodes.extend(child_nodes);
            }
        }

        all_nodes
    }

    pub fn render(&mut self, wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, scene: &Box<crate::state::scene::scene::Scene>) -> Vec<RenderResultForCamera>
    {
        let all_nodes = Scene::list_all_child_nodes(&scene.nodes, true);

        let mut nodes_read = vec![];
        let mut materials = vec![];
        let mut materials_read = vec![];
        let mut meshes = vec![];
        let mut meshes_read = vec![];

        let default_material = scene.get_default_material();

        if scene.get_default_material().is_none()
        {
            console_warning!("default material not found -> please do not delete it");
            return vec![];
        }

        let default_material_arc = default_material.unwrap();
        let default_material = &default_material_arc.read().unwrap();

        for node in &all_nodes
        {
            let read_node = node.read().unwrap();
            let mat = read_node.find_component::<MaterialComponent>();
            let node_meshes = read_node.get_meshes_with_mesh_resource();

            if node_meshes.len() > 0
            {
                nodes_read.push(read_node);
                meshes.push(node_meshes);

                if let Some(mat) = mat
                {
                    materials.push(mat);
                }
                else
                {
                    materials.push(default_material_arc.clone());
                }
            }
        }

        for material in &materials
        {
            let material_read = material.read().unwrap();
            materials_read.push(material_read);
        }

        for mesh in &meshes
        {
            let mesh_read: Vec<_> = mesh.iter().map(|mesh_item| mesh_item.read().unwrap()).collect();
            meshes_read.push(mesh_read);
        }

        // solid_objects and transparent_objects
        let mut render_groups: Vec<(Vec<RenderData>, Vec<RenderData>)> = vec![];
        let mut rendering_group_map: HashMap<i64, usize> = HashMap::new();

        for (i, material) in materials_read.iter().enumerate()
        {
            let mat;
            if !material.is_enabled()
            {
                mat = default_material;
            }
            else
            {
                mat = material;
            }

            let node = nodes_read.get(i).unwrap();
            let meshes = meshes_read.get(i).unwrap();

            if meshes.len() == 0 || node.instances.get_ref().len() == 0
            {
                continue;
            }

            let mut bounding_sphere = None;

            // ***** get center for depth sorting (alpha blending)
            if self.distance_sorting || self.frustum_culling
            {
                if let Some(instance_render_item) = node.instance_render_item.as_ref()
                {
                    let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);
                    bounding_sphere = node.get_bounding_sphere_for_all_instances(&instance_buffer.transformations);
                }
            }

            let has_transparency;
            {
                let mat = mat.as_any().downcast_ref::<MaterialComponent>().unwrap();
                has_transparency = mat.has_transparency();
            }

            let node = nodes_read.get(i).unwrap();
            let render_group_id = node.settings.render_group_id;

            let item = RenderData
            {
                node,
                material: mat,
                meshes: meshes,

                has_transparency: has_transparency,
                alpha_index: node.settings.alpha_index,
                middle: bounding_sphere.map(|(center, _)| center),
                radius: bounding_sphere.map(|(_, radius)| radius),
            };

            let i = *rendering_group_map.entry(render_group_id).or_insert_with(||
            {
                render_groups.push((vec![], vec![]));
                render_groups.len() - 1
            });

            let (solid_objects, transparent_objects) = render_groups.get_mut(i).unwrap();

            if has_transparency
            {
                transparent_objects.push(item);
            }
            else
            {
                solid_objects.push(item);
            }
        }

        let mut render_results = vec![];

        // create render results
        for cam in &scene.cameras
        {
            if cam.enabled
            {
                render_results.push(RenderResultForCamera::new());
            }
        }

        // x-ray shows occluded geometry -> occlusion culling would remove exactly that
        let occlusion_active = self.occlusion_culling && self.occlusion_supported && !self.xray_mode;

        // occlusion culling was just (re-)enabled -> reset the gpu culling state to "everything visible"
        if occlusion_active && !self.occlusion_was_active
        {
            for cam in &scene.cameras
            {
                if let Some(indirect_args) = cam.indirect_args_render_item.as_ref()
                {
                    get_render_item::<IndirectArgsBuffers>(indirect_args).reset_full_visible(wgpu, &self.draw_slots.slots);
                }

                if let Some(visibility_buffer) = cam.visibility_buffer_render_item.as_ref()
                {
                    get_render_item::<VisibilityBuffer>(visibility_buffer).reset_all_visible(wgpu);
                }
            }
        }
        self.occlusion_was_active = occlusion_active;

        // read back visibility results (async stats - a few frames behind, never blocks)
        if occlusion_active
        {
            self.read_back_visibility_results(wgpu, &scene.cameras, &mut render_results);
        }

        // read back gpu pass timings from the previous frame
        if let Some(gpu_timer) = self.gpu_timer.as_mut()
        {
            gpu_timer.read_back_results(wgpu);
        }

        // take out the gpu timer to keep self borrowable for the render functions
        let mut gpu_timer = self.gpu_timer.take();

        // ********** shadow maps **********
        // render all shadow views (directional cascades, spot, point faces) into the shadow atlas
        let (shadow_views, shadow_draw_calls) = self.render_shadows(wgpu, encoder, scene, &render_groups, gpu_timer.as_mut());

        self.shadow_views = shadow_views;
        self.shadow_draw_calls = shadow_draw_calls;

        // render for each camera
        let mut i = 0;
        for (_cam_index, cam) in scene.cameras.iter().enumerate()
        {
            if !cam.enabled { continue; }

            let render_result = &mut render_results[i];

            let cam_data = cam.get_data();
            let cam_pos = cam_data.eye_pos;
            let cam_culling_mask = cam_data.culling_mask;

            // ********** layer culling **********
            // filter nodes whose layer_mask does not intersect with the camera's culling_mask
            let render_groups_layer_filtered: Vec<_> = render_groups.iter().map(|(solid_objects, transparent_objects)|
            {
                let solid_culled: Vec<_> = solid_objects.iter().filter(|item|
                {
                    (item.node.settings.layer_mask & cam_culling_mask) != 0
                }).copied().collect();
                let transparent_culled: Vec<_> = transparent_objects.iter().filter(|item|
                {
                    (item.node.settings.layer_mask & cam_culling_mask) != 0
                }).copied().collect();

                (solid_culled, transparent_culled)
            }).collect();

            // ********** frustum culling **********
            let mut render_groups_frustum_culled = if self.frustum_culling
            {
                render_groups_layer_filtered.iter().map(|(solid_objects, transparent_objects)|
                {
                    let solid_culled: Vec<_> = solid_objects.iter().filter(|item|
                    {
                        if let (Some(center), Some(radius)) = (item.middle.as_ref(), item.radius)
                        {
                            cam.is_sphere_in_frustum(center, radius) || !item.node.settings.frustum_culling
                        }
                        else
                        {
                            false
                        }
                    }).copied().collect();
                    let transparent_culled: Vec<_> = transparent_objects.iter().filter(|item|
                    {
                        if let (Some(center), Some(radius)) = (item.middle.as_ref(), item.radius)
                        {
                            cam.is_sphere_in_frustum(center, radius) || !item.node.settings.frustum_culling
                        }
                        else
                        {
                            false
                        }
                    }).copied().collect();

                    (solid_culled, transparent_culled)
                }).collect::<Vec<_>>()
            }
            else
            {
                render_groups_layer_filtered.clone()
            };

            // objects dropped by the cpu frustum culling (per camera)
            let layer_filtered_count: usize = render_groups_layer_filtered.iter().map(|(solid, transparent)| solid.len() + transparent.len()).sum();
            let frustum_culled_count: usize = render_groups_frustum_culled.iter().map(|(solid, transparent)| solid.len() + transparent.len()).sum();
            render_result.objects_frustum_culled = (layer_filtered_count - frustum_culled_count) as u32;

            // ********** alpha / distance sorting **********
            if self.distance_sorting
            {
                for (solid_objects, transparent_objects) in &mut render_groups_frustum_culled
                {
                    // sort solid objects front-to-back for early-z / occlusion culling
                    if occlusion_active
                    {
                        solid_objects.sort_by(|a, b|
                        {
                            let a_middle = a.middle.unwrap_or(Point3::origin());
                            let b_middle = b.middle.unwrap_or(Point3::origin());

                            let a_dist = distance_squared(&a_middle, &cam_pos);
                            let b_dist = distance_squared(&b_middle, &cam_pos);

                            // front-to-back: smaller distance first
                            a_dist.partial_cmp(&b_dist).unwrap()
                        });
                    }

                    // sort transparent objects back-to-front for alpha blending
                    transparent_objects.sort_by(|a, b|
                    {
                        if a.has_transparency != b.has_transparency
                        {
                            b.has_transparency.cmp(&a.has_transparency)
                        }
                        else if a.alpha_index != b.alpha_index
                        {
                            a.alpha_index.cmp(&b.alpha_index)
                        }
                        else
                        {
                            let a_middle = a.middle.unwrap_or(Point3::origin());
                            let b_middle = b.middle.unwrap_or(Point3::origin());

                            // we do not need the exact distance here - squared is fine
                            let a_dist = distance_squared(&a_middle, &cam_pos);
                            let b_dist = distance_squared(&b_middle, &cam_pos);

                            // back-to-front: larger distance first
                            b_dist.partial_cmp(&a_dist).unwrap()
                        }
                    });
                }
            }

            let clear;
            if i == 0 { clear = true; } else { clear = false; }


            // get bind groups
            let bind_group_render_item = cam.bind_group_render_item.as_ref().unwrap();
            let bind_group_render_item = get_render_item::<LightCamSceneBindGroup>(bind_group_render_item);

            let indirect_args = if occlusion_active && cam.indirect_args_render_item.is_some() && cam.hzb_occlusion_bind_group_render_item.is_some()
            {
                Some(get_render_item::<IndirectArgsBuffers>(cam.indirect_args_render_item.as_ref().unwrap()))
            }
            else
            {
                None
            };

            if let Some(indirect_args) = indirect_args
            {
                // ********** two-pass hzb occlusion culling **********
                // pass 1 renders the objects which were visible in the previous frame (indirect
                // draws - the instance counts are masked on the gpu), the hzb is built from their
                // depth, the occlusion check tests all objects against it and pass 2 renders the
                // objects which became visible this frame. the visibility never leaves the gpu
                // -> no cpu readback/stall (the stats readback is async)

                // (occluder solids: depth test + write, other solids, transparents) per render group
                let mut split_groups: Vec<(Vec<RenderData>, Vec<RenderData>, Vec<RenderData>)> = vec![];
                for (solid_objects, transparent_objects) in &render_groups_frustum_culled
                {
                    let (occluders, other): (Vec<_>, Vec<_>) = solid_objects.iter().copied().partition(|item| item.node.settings.depth_test && item.node.settings.depth_write);
                    split_groups.push((occluders, other, transparent_objects.clone()));
                }

                // ********** pass 1: depth (objects visible in the previous frame) **********
                let pass1_batches: Vec<(&Vec<RenderData>, Option<&wgpu::Buffer>)> = split_groups.iter().map(|(occluders, _, _)| (occluders, Some(&indirect_args.args_visible))).collect();

                render_result.draw_calls += self.render_depth(wgpu, view, encoder, &pass1_batches, cam_data, &bind_group_render_item.bind_group, clear, gpu_timer.as_mut());

                // ********** hzb + occlusion check (from the pass 1 depth) **********
                // the hzb block spans multiple passes: the depth export pass writes the begin
                // timestamp and the occlusion check pass writes the end timestamp
                let hzb_timer_segment = gpu_timer.as_mut().and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Hzb));

                render_result.draw_calls += self.create_hzb(wgpu, encoder, cam, hzb_timer_segment);
                self.hzb_occlusion_culling(wgpu, encoder, cam, hzb_timer_segment);

                // ********** pass 2: depth (objects which became visible this frame) **********
                // completes the opaque depth before ssao samples it - otherwise disoccluded
                // objects would be shaded with the occlusion of the geometry behind them
                let pass2_batches: Vec<(&Vec<RenderData>, Option<&wgpu::Buffer>)> = split_groups.iter().map(|(occluders, _, _)| (occluders, Some(&indirect_args.args_new))).collect();

                render_result.draw_calls += self.render_depth(wgpu, view, encoder, &pass2_batches, cam_data, &bind_group_render_item.bind_group, false, gpu_timer.as_mut());

                // ********** ssao (from the completed opaque depth) **********
                if self.ssao_supported && self.ssao_enabled && self.ssao_strength > 0.0
                {
                    render_result.draw_calls += self.render_ssao(wgpu, encoder, cam, cam_data, clear, gpu_timer.as_mut());
                }

                // ********** color: all currently visible objects **********
                // after the occlusion check args_visible holds ALL currently visible objects
                // (including the newly visible ones) -> a single color pass draws everything
                let mut color_batches: Vec<(&Vec<RenderData>, Option<&wgpu::Buffer>)> = vec![];
                for (occluders, other_solids, transparents) in &split_groups
                {
                    color_batches.push((occluders, Some(&indirect_args.args_visible)));
                    color_batches.push((other_solids, Some(&indirect_args.args_visible)));
                    color_batches.push((transparents, Some(&indirect_args.args_visible)));
                }

                render_result.draw_calls += self.render_color(wgpu, view, msaa_view, encoder, &color_batches, cam_data, &bind_group_render_item.bind_group, clear, gpu_timer.as_mut());
            }
            else
            {
                // ********** depth pre-pass **********
                // solids only: the ssao pass samples this depth and expects the occlusion of
                // the opaque geometry (a transparent pane must not darken the floor behind it)
                let mut solid_data = Vec::with_capacity(materials_read.len());
                let mut transparent_data = Vec::with_capacity(materials_read.len());
                for (solid_objects, transparent_objects) in &render_groups_frustum_culled
                {
                    solid_data.extend(solid_objects.iter().cloned());
                    transparent_data.extend(transparent_objects.iter().cloned());
                }

                let depth_batches: [(&Vec<RenderData>, Option<&wgpu::Buffer>); 1] = [(&solid_data, None)];

                render_result.draw_calls += self.render_depth(wgpu, view, encoder, &depth_batches, cam_data, &bind_group_render_item.bind_group, clear, gpu_timer.as_mut());

                // ********** ssao (from the depth pre-pass) **********
                if self.ssao_supported && self.ssao_enabled && self.ssao_strength > 0.0
                {
                    render_result.draw_calls += self.render_ssao(wgpu, encoder, cam, cam_data, clear, gpu_timer.as_mut());
                }

                // ********** color pass **********
                let color_batches: [(&Vec<RenderData>, Option<&wgpu::Buffer>); 2] = [(&solid_data, None), (&transparent_data, None)];

                render_result.draw_calls += self.render_color(wgpu, view, msaa_view, encoder, &color_batches, cam_data, &bind_group_render_item.bind_group, clear, gpu_timer.as_mut());
            }

            i += 1;
        }

        // resolve the gpu timestamps into the readback buffer (read back in the next frame)
        if let Some(gpu_timer) = gpu_timer.as_mut() { gpu_timer.resolve(encoder); }
        self.gpu_timer = gpu_timer;

        render_results
    }

    pub fn create_hzb(&mut self, _wgpu: &mut WGpu, encoder: &mut CommandEncoder, cam: &Camera, timer_segment: Option<GpuTimerSegment>) -> u32
    {
        let mut draw_calls: u32 = 0;

        let hzb_texture = get_render_item::<Texture>(cam.hzb_texture_render_item.as_ref().unwrap());
        let (hzb_width, hzb_height) = (hzb_texture.width, hzb_texture.height);

        // *********** depth export pass (depth -> hzb mip 0) **********
        // the hzb texture covers exactly the camera viewport - the bind group
        // remaps the sampling to the viewport region of the depth texture
        {
            let depth_export_bind_group = get_render_item::<DepthExportBindGroup>(cam.depth_export_bind_group_render_item.as_ref().unwrap());

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("Depth Export Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment
                {
                    view: &hzb_texture.get_view(),
                    resolve_target: None,
                    ops: wgpu::Operations
                    {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: timer_segment.map(|timer_segment| timer_segment.begin_render_writes()),
                multiview_mask: None,
            });

            let pipeline = self.render_pipelines[RenderPipelineType::DepthExport as usize].get();

            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &depth_export_bind_group.bind_group, &[]);

            pass.draw(0..3, 0..1); // fullscreen triangle
            draw_calls += 1;
        }

        // ************ generate HZB mipmaps **********
        let hzb_downsample_bind_group = get_render_item::<HZBDownsampleBindGroup>(cam.hzb_downsample_bind_group_render_item.as_ref().unwrap());
        let pipeline = self.compute_pipelines[ComputePipelineType::HzbDownsample as usize].get();

        let workgroup_size: u32 = 8;

        for (level, bind_group) in hzb_downsample_bind_group.bind_groups.iter().enumerate()
        {
            let dst_width  = (hzb_width  >> (level + 1)).max(1);
            let dst_height = (hzb_height >> (level + 1)).max(1);

            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor
            {
                label: Some("HZB Downsample"),
                timestamp_writes: None,
            });

            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, bind_group, &[]);

            let wg_x = (dst_width  + (workgroup_size - 1)) / workgroup_size;
            let wg_y = (dst_height + (workgroup_size - 1)) / workgroup_size;

            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        draw_calls
    }

    pub fn hzb_occlusion_culling(&mut self, _wgpu: &mut WGpu, encoder: &mut CommandEncoder, cam: &Camera, timer_segment: Option<GpuTimerSegment>)
    {
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor
            {
                label: Some("Occlusion Culling Pass"),
                timestamp_writes: timer_segment.map(|timer_segment| timer_segment.end_compute_writes()),
            });

            let pipeline = self.compute_pipelines[ComputePipelineType::HzbOcclusionCheck as usize].get();

            let bind_group = get_render_item::<HZBOcclusionCheckBindGroup>(cam.hzb_occlusion_bind_group_render_item.as_ref().unwrap());

            compute_pass.set_pipeline(&pipeline);
            compute_pass.set_bind_group(0, &bind_group.bind_group, &[]);

            // Workgroup size (same as in shader)
            let workgroup_size = 64u32;
            let num_wg = (self.hzb_cull_buffer.num_objects as u32 + workgroup_size - 1) / workgroup_size;

            compute_pass.dispatch_workgroups(num_wg, 1, 1);
        }

        let visibility_buffer = get_render_item::<VisibilityBuffer>(cam.visibility_buffer_render_item.as_ref().unwrap());
        let num_objects = self.hzb_cull_buffer.num_objects;

        // current visibility becomes the "previous frame" input of the next frame
        visibility_buffer.copy_current_to_prev(encoder, num_objects);

        // async stats readback (non-blocking, skipped while a readback is still in flight)
        visibility_buffer.record_readback_copy(encoder, num_objects);
    }

    // non-blocking: fills the render results with the latest read back visibility (a few frames behind)
    pub fn read_back_visibility_results(&mut self, wgpu: &mut WGpu, cameras: &std::vec::Vec<Box<Camera>>, render_results: &mut Vec<RenderResultForCamera>)
    {
        let mut result_index = 0;
        for cam in cameras.iter()
        {
            if !cam.enabled { continue; }

            let render_result = &mut render_results[result_index];
            render_result.camera_id = cam.id;
            result_index += 1;

            if cam.visibility_buffer_render_item.is_none() { continue; }

            let visibility_buffer = get_render_item::<VisibilityBuffer>(cam.visibility_buffer_render_item.as_ref().unwrap());
            visibility_buffer.update_readback(wgpu);

            for res in visibility_buffer.latest_results()
            {
                if res.visible > 0
                {
                    render_result.objects_visible.push(res.object_id);
                }
                else
                {
                    render_result.objects_invisible.push(res.object_id);
                }
            }
        }
    }

    // averaged gpu time per pass block (None if the adapter does not support timestamp queries)
    pub fn gpu_pass_times(&self) -> Option<GpuPassTimes>
    {
        self.gpu_timer.as_ref().map(|gpu_timer| gpu_timer.pass_times())
    }

    // returns the number of rendered shadow views and the number of shadow draw calls
    pub fn render_shadows(&self, wgpu: &mut WGpu, encoder: &mut CommandEncoder, scene: &Box<crate::state::scene::scene::Scene>, render_groups: &Vec<(Vec<RenderData>, Vec<RenderData>)>, gpu_timer: Option<&mut GpuTimer>) -> (u32, u32)
    {
        if !self.shadow_enabled
        {
            return (0, 0);
        }

        let max_lights = scene.get_data().max_lights as usize;
        let lights = scene.lights.get_ref();

        // directional cascades are fitted to the first enabled camera
        let cam_data = scene.cameras.iter().find(|cam| cam.enabled).map(|cam| cam.get_data());

        let shadow_views = shadow::compute_shadow_views(lights, max_lights, cam_data, self.shadow.size(), self.shadow_max_distance);

        if shadow_views.is_empty()
        {
            return (0, 0);
        }

        self.shadow.write_views(wgpu, &shadow_views);

        // only views which fit into the shadow atlas are rendered
        let views_to_render: Vec<_> = shadow_views.iter().filter(|shadow_view| shadow_view.layer < self.shadow.layers()).collect();

        // the begin timestamp is written by the first shadow pass and the end timestamp by the last one
        let timer_segment = if views_to_render.is_empty() { None } else { gpu_timer.and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Shadow)) };

        let mut draw_calls: u32 = 0;

        for (i, shadow_view) in views_to_render.iter().copied().enumerate()
        {
            let timestamp_writes = timer_segment.and_then(|timer_segment| timer_segment.render_writes_for_pass(i, views_to_render.len()));

            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
                {
                    view: self.shadow.get_layer_view(shadow_view.layer),
                    depth_ops: Some(wgpu::Operations
                    {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // transparent objects do not cast shadows
            for (solid_objects, _transparent_objects) in render_groups
            {
                draw_calls += self.draw_phase(&mut render_pass, &DrawPhase::Shadow { shadow_view }, solid_objects, None);
            }
        }

        (views_to_render.len() as u32, draw_calls)
    }

    // two fullscreen passes: ssao (depth -> raw occlusion) + blur (raw -> blurred occlusion)
    // the blurred result is sampled by the color pass via the light/cam/scene bind group
    pub fn render_ssao(&mut self, _wgpu: &mut WGpu, encoder: &mut CommandEncoder, cam: &Camera, cam_data: &CameraData, clear: bool, gpu_timer: Option<&mut GpuTimer>) -> u32
    {
        if cam.ssao_bind_group_render_item.is_none()
        {
            return 0;
        }

        let ssao_bind_group = get_render_item::<SsaoBindGroup>(cam.ssao_bind_group_render_item.as_ref().unwrap());

        // both ssao passes always run together -> begin/end timestamps are always written
        let timer_segment = gpu_timer.and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Ssao));

        // whole pixels (top-left origin) - must match the viewport in the ssao uniform
        let [x, y, width, height] = cam_data.viewport_px();

        // the first camera clears the (surface sized) targets - 1.0 = no occlusion
        let load_op = if clear { wgpu::LoadOp::Clear(wgpu::Color::WHITE) } else { wgpu::LoadOp::Load };

        // ********** ssao pass **********
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("ssao pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment
                {
                    view: &self.ssao_texture.get_view(),
                    resolve_target: None,
                    ops: wgpu::Operations
                    {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: timer_segment.map(|timer_segment| timer_segment.begin_render_writes()),
                multiview_mask: None,
            });

            // set viewport uses top-left origin (we are using bottom-left origin)
            pass.set_viewport(x, y, width, height, 0.0, 1.0);

            pass.set_pipeline(&self.render_pipelines[RenderPipelineType::Ssao as usize].get());
            pass.set_bind_group(0, &ssao_bind_group.ssao_bind_group, &[]);

            pass.draw(0..3, 0..1); // fullscreen triangle
        }

        // ********** blur pass **********
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
            {
                label: Some("ssao blur pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment
                {
                    view: &self.ssao_blur_texture.get_view(),
                    resolve_target: None,
                    ops: wgpu::Operations
                    {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: timer_segment.map(|timer_segment| timer_segment.end_render_writes()),
                multiview_mask: None,
            });

            pass.set_viewport(x, y, width, height, 0.0, 1.0);

            pass.set_pipeline(&self.render_pipelines[RenderPipelineType::SsaoBlur as usize].get());
            pass.set_bind_group(0, &ssao_bind_group.blur_bind_group, &[]);

            pass.draw(0..3, 0..1); // fullscreen triangle
        }

        2
    }

    // batches: (nodes, optional indirect args buffer) - indirect batches are drawn gpu-driven
    pub fn render_depth(&mut self, _wgpu: &mut WGpu, view: &TextureView, encoder: &mut CommandEncoder, batches: &[(&Vec<RenderData>, Option<&wgpu::Buffer>)], cam_data: &CameraData, light_cam_bind_group: &BindGroup, clear: bool, gpu_timer: Option<&mut GpuTimer>) -> u32
    {
        let timer_segment = gpu_timer.and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Depth));

        let mut clear_color = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        let mut clear_depth = wgpu::LoadOp::Clear(1.0);

        if !clear
        {
            clear_color = wgpu::LoadOp::Load;
            clear_depth = wgpu::LoadOp::Load;
        }

        // todo: replace with internal texture?
        let render_pass_view = view;

        let color_attachments: &[Option<RenderPassColorAttachment>] =
        &[
            Some(wgpu::RenderPassColorAttachment
            {
                view: render_pass_view,
                resolve_target: None,
                ops: wgpu::Operations
                {
                    load: clear_color,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })
        ];

        // TODO get rid of this
        /*
        if !self.depth_pipe.as_ref().unwrap().fragment_attachment
        {
            color_attachments = &[];
        }
        */

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("depth pass"),
            color_attachments: color_attachments,
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_pass_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: clear_depth,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timer_segment.map(|timer_segment| timer_segment.full_render_writes()),
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // whole pixels, top-left origin (the viewport values use bottom-left origin)
        let [x, y, width, height] = cam_data.viewport_px();
        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        let mut draw_calls = 0;
        for (nodes, indirect_args) in batches
        {
            draw_calls += self.draw_phase(&mut render_pass, &DrawPhase::Depth { light_cam_bind_group }, nodes, *indirect_args);
        }

        draw_calls
    }

    // batches: (nodes, optional indirect args buffer) - indirect batches are drawn gpu-driven
    pub fn render_color(&mut self, _wgpu: &mut WGpu, view: &TextureView, msaa_view: &Option<TextureView>, encoder: &mut CommandEncoder, batches: &[(&Vec<RenderData>, Option<&wgpu::Buffer>)], cam_data: &CameraData, light_cam_bind_group: &BindGroup, clear: bool, gpu_timer: Option<&mut GpuTimer>) -> u32
    {
        let timer_segment = gpu_timer.and_then(|gpu_timer| gpu_timer.begin_segment(GpuTimerPass::Color));

        let mut render_pass_view = view;
        let mut render_pass_resolve_target = None;
        if msaa_view.is_some()
        {
            render_pass_view = msaa_view.as_ref().unwrap();
            render_pass_resolve_target = Some(view);
        }

        let mut clear_color = wgpu::LoadOp::Clear(self.clear_color);
        let mut clear_depth = wgpu::LoadOp::Clear(1.0);

        if !clear
        {
            clear_color = wgpu::LoadOp::Load;
            clear_depth = wgpu::LoadOp::Load;
        }

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor
        {
            label: Some("color pass"),
            color_attachments:
            &[
                Some(wgpu::RenderPassColorAttachment
                {
                    view: render_pass_view,
                    resolve_target: render_pass_resolve_target,
                    ops: wgpu::Operations
                    {
                        load: clear_color,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment
            {
                view: &self.depth_buffer_texture.get_view(),
                depth_ops: Some(wgpu::Operations
                {
                    load: clear_depth,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timer_segment.map(|timer_segment| timer_segment.full_render_writes()),
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // whole pixels, top-left origin (the viewport values use bottom-left origin)
        let [x, y, width, height] = cam_data.viewport_px();
        render_pass.set_viewport(x, y, width, height, 0.0, 1.0);

        let mut draw_calls = 0;
        for (nodes, indirect_args) in batches
        {
            draw_calls += self.draw_phase(&mut render_pass, &DrawPhase::Color { light_cam_bind_group }, nodes, *indirect_args);
        }

        draw_calls
    }

    // indirect_args: when set, the draws are recorded as indirect draws - the occlusion check
    // compute pass writes the instance counts (0 = culled) into the args buffer
    fn draw_phase<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, phase: &DrawPhase, nodes: &'a Vec<RenderData>, indirect_args: Option<&'a wgpu::Buffer>) -> u32
    {
        let mut draw_calls: u32 = 0;

        for data in nodes
        {
            let node = data.node;
            let meshes = data.meshes;
            let mat = data.material;

            if !node.settings.visible
            {
                continue;
            }

            if meshes.len() == 0
            {
                continue;
            }

            let material_component = mat.as_any().downcast_ref::<MaterialComponent>();

            // shadow pass: only shadow casting materials + per-view culling
            if let DrawPhase::Shadow { shadow_view } = phase
            {
                if !material_component.map(|m| m.get_data().cast_shadow).unwrap_or(true)
                {
                    continue;
                }

                // per-view culling via bounding sphere (if available)
                if let (Some(center), Some(radius)) = (data.middle.as_ref(), data.radius)
                {
                    if !shadow_view.intersects_sphere(center, radius)
                    {
                        continue;
                    }
                }
            }

            let material_render_item = mat.get_base().render_item.as_ref();
            let material_render_item = get_render_item::<MaterialBuffer>(material_render_item.as_ref().unwrap());
            let material_bind_group = material_render_item.bind_group.as_ref().unwrap();

            let material_allow_xray = material_component.map(|m| m.get_data().allow_xray).unwrap_or(true);

            // the mesh index maps to the draw slot of the (node, mesh) pair - it has to count
            // all meshes of the node (including skipped ones) to stay aligned with the slot table
            for (mesh_index, mesh) in meshes.iter().enumerate()
            {
                let mesh = mesh.as_any().downcast_ref::<Mesh>().unwrap();

                if !mesh.get_base().is_enabled
                {
                    continue;
                }

                //if let Some(render_item) = mesh.get_base().render_item.as_ref()
                // existence of mesh_resource is guaranteed
                if let Some(render_item) = mesh.mesh_resource.as_ref().unwrap().read().unwrap().render_item.as_ref()
                {
                    let vertex_buffer = get_render_item::<VertexBuffer>(&render_item);

                    if let Some(instance_render_item) = node.instance_render_item.as_ref()
                    {
                        let instance_buffer = get_render_item::<InstanceBuffer>(instance_render_item);

                        // ********** pipeline + phase specific bind groups **********
                        let skeleton_bind_group_slot;
                        match phase
                        {
                            DrawPhase::Shadow { shadow_view } =>
                            {
                                pass.set_pipeline(&self.render_pipelines[RenderPipelineType::Shadow as usize].get());

                                // per-view light matrix via dynamic offset
                                let dynamic_offset = shadow_view.layer * shadow::SHADOW_VIEW_UNIFORM_STRIDE as u32;
                                pass.set_bind_group(0, &self.shadow.caster_bind_group, &[dynamic_offset]);

                                skeleton_bind_group_slot = 1;
                            },
                            DrawPhase::Depth { light_cam_bind_group } | DrawPhase::Color { light_cam_bind_group } =>
                            {
                                let color_pipeline = matches!(phase, DrawPhase::Color { .. });

                                // x-ray mode forces no depth-write on the color pass so back faces / occluded objects remain visible
                                // materials with allow_xray=false (gizmos, grid, etc.) keep their normal pipeline so they stay on top
                                let xray_color = color_pipeline && self.xray_mode && material_allow_xray;

                                if node.settings.depth_test && node.settings.depth_write
                                {
                                    if color_pipeline
                                    {
                                        let pipe = if xray_color { RenderPipelineType::ColorNoWrite } else { RenderPipelineType::Color };
                                        pass.set_pipeline(&self.render_pipelines[pipe as usize].get());
                                    }
                                    else
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::Depth as usize].get());
                                    }
                                }
                                else if !node.settings.depth_test && node.settings.depth_write
                                {
                                    if color_pipeline
                                    {
                                        let pipe = if xray_color { RenderPipelineType::ColorNoWriteNoCompare } else { RenderPipelineType::ColorNoCompare };
                                        pass.set_pipeline(&self.render_pipelines[pipe as usize].get());
                                    }
                                    else
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::DepthNoCompare as usize].get());
                                    }
                                }
                                else if node.settings.depth_test && !node.settings.depth_write
                                {
                                    if color_pipeline
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::ColorNoWrite as usize].get());
                                    }
                                    else
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::DepthNoWrite as usize].get());
                                    }
                                }
                                else
                                {
                                    if color_pipeline
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::ColorNoWriteNoCompare as usize].get());
                                    }
                                    else
                                    {
                                        pass.set_pipeline(&self.render_pipelines[RenderPipelineType::DepthNoWriteNoCompare as usize].get());
                                    }
                                }

                                pass.set_bind_group(0, material_bind_group, &[]);
                                pass.set_bind_group(1, *light_cam_bind_group, &[]);

                                skeleton_bind_group_slot = 2;
                            }
                        }

                        // skeleton
                        let skeleton_morph_target_render_item = node.skeleton_morph_target_bind_group_render_item.as_ref();
                        if let Some(skeleton_morph_target_render_item) = skeleton_morph_target_render_item
                        {
                            let skeleton_morph_target_render_item = get_render_item::<SkeletonMorphTargetBindGroup>(skeleton_morph_target_render_item);
                            pass.set_bind_group(skeleton_bind_group_slot, &skeleton_morph_target_render_item.as_ref().bind_group, &[]);
                        }
                        else
                        {
                            pass.set_bind_group(skeleton_bind_group_slot, &self.empty_skeleton_morph_group.bind_group, &[]);
                        }

                        pass.set_vertex_buffer(0, vertex_buffer.get_vertex_buffer().slice(..));

                        // instancing
                        pass.set_vertex_buffer(1, instance_buffer.get_buffer().slice(..));

                        pass.set_index_buffer(vertex_buffer.get_index_buffer().slice(..), wgpu::IndexFormat::Uint32);

                        match indirect_args
                        {
                            Some(args_buffer) =>
                            {
                                // gpu-driven draw: the occlusion check writes the instance count (0 = culled)
                                // the bounds check guards against a stale slot table (mesh list changed
                                // between the slot build and this draw)
                                if let Some((first_slot, slot_count)) = self.draw_slots.slot_map.get(&node.id)
                                {
                                    if (mesh_index as u32) < *slot_count
                                    {
                                        let slot = *first_slot as u64 + mesh_index as u64;
                                        pass.draw_indexed_indirect(args_buffer, slot * DRAW_INDEXED_ARGS_SIZE);

                                        draw_calls += 1;
                                    }
                                }
                            },
                            None =>
                            {
                                pass.draw_indexed(0..vertex_buffer.get_index_count(), 0, 0..instance_buffer.get_count() as _);

                                draw_calls += 1;
                            }
                        }
                    }
                }
            }
        }

        draw_calls
    }
}

pub fn render_scene_offscreen_to_image(wgpu: &mut WGpu, state: &mut State, scene: &mut Box<crate::state::scene::scene::Scene>, width: u32, height: u32) -> image::DynamicImage
{
    // ensure a render item exists
    if scene.render_item.is_none()
    {
        let samples = *(state.rendering.msaa.get_ref());
        let render_item = Scene::new(wgpu, state, scene, samples);
        scene.render_item = Some(Box::new(render_item));
    }

    // update first (before resizing) to ensure render item is up-to-date and can handle resizing properly
    {
        let mut render_item = scene.render_item.take();
        let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
        render_scene.update(wgpu, state, scene);
        scene.render_item = render_item;
    }

    // switch depth buffer / camera / hzb to the target resolution
    scene.update_resolution(width, height);
    {
        let mut render_item = scene.render_item.take();
        let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
        render_scene.resize(wgpu, scene, width, height);
        scene.render_item = render_item;
    }

    // update + render off-screen
    let mut render_item = scene.render_item.take();
    let render_scene = get_render_item_mut::<Scene>(render_item.as_mut().unwrap());
    render_scene.distance_sorting = state.rendering.distance_sorting;
    render_scene.frustum_culling = state.rendering.frustum_culling;
    render_scene.occlusion_culling = false;
    render_scene.update(wgpu, state, scene);

    let (buffer_dimensions, output_buffer, texture, view, msaa_view) = wgpu.start_offscreen_render(Some((width, height)));
    let mut encoder = wgpu.create_command_encoder();
    render_scene.render(wgpu, &view, &msaa_view, &mut encoder, scene);
    let img = wgpu.end_offscreen_render(buffer_dimensions, output_buffer, texture, encoder);

    scene.render_item = render_item;

    img
}
