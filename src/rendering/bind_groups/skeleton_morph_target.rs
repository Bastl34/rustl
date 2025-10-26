#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bind_groups::uniform, morph_target::MorphTarget, skeleton::SkeletonBuffer, wgpu::WGpu}, state::helper::render_item::RenderItem};

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
        let morph_layout_entry = MorphTarget::get_bind_group_layout_entry(2);

        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries:
            &[
                // skeleton
                uniform::uniform_bind_group_layout_entry(0, true, false),

                // morph targets buffer
                uniform::uniform_bind_group_layout_entry(1, true, false),

                // morph targets (texture array)
                morph_layout_entry
            ],
            label: Some("skeleton_morph_target_bind_group_layout"),
        });

        bind_group_layout
    }

    pub fn new(wgpu: &mut WGpu, name: &str, skeleton_buffer: &SkeletonBuffer, morph_target: &MorphTarget) -> SkeletonMorphTargetBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu);

        let morph_target_tex_bind_group = morph_target.get_bind_group_entry(2);

        let bind_group_name = format!("{} skeleton_morph_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                // skeleton
                wgpu::BindGroupEntry { binding: 0, resource: skeleton_buffer.get_buffer().as_entire_binding() },

                // morph targets buffer
                wgpu::BindGroupEntry { binding: 1, resource: morph_target.get_buffer().as_entire_binding() },

                // morph targets (texture array)
                morph_target_tex_bind_group.clone(), // ????
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