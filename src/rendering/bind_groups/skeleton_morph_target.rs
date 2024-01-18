use wgpu::{BindGroupLayout, BindGroup};

use crate::{rendering::{light::LightBuffer, camera::CameraBuffer, wgpu::WGpu, uniform, scene::Scene, morph_target::MorphTarget, skeleton::SkeletonBuffer}, state::helper::render_item::RenderItem, render_item_impl_default};

pub struct SkeletonMorphTargetBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for SkeletonMorphTargetBindGroup
{
    render_item_impl_default!();
}

impl SkeletonMorphTargetBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu) -> BindGroupLayout
    {
        let morph_layout_entries = MorphTarget::get_bind_group_layout_entries(1);

        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                // skeleton
                uniform::uniform_bind_group_layout_entry(0, true, true),

                // morph targets
                morph_layout_entries[0],
                morph_layout_entries[1],
            ],
            label: Some("skeleton_morph_target_bind_group_layout"),
        });

        bind_group_layout
    }

    pub fn new(wgpu: &mut WGpu, name: &str, skeleton_buffer: &SkeletonBuffer, morph_target: &MorphTarget) -> SkeletonMorphTargetBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        let morph_target_tex_bind_groups = morph_target.get_bind_group_entries(1);

        let bind_group_name = format!("{} light_camera_scene_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                // skeleton
                uniform::uniform_bind_group(0, &skeleton_buffer.get_buffer()),

                // morph targets
                morph_target_tex_bind_groups[0].clone(), // ????
                morph_target_tex_bind_groups[1].clone(), // ????
                //uniform::uniform_bind_group(1, &scene_buffer.get_buffer()),
                //uniform::uniform_bind_group(1, &scene_buffer.get_buffer()),
            ],
            label: Some(bind_group_name.as_str()),
        });

        SkeletonMorphTargetBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}