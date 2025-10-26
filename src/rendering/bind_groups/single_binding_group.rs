#![allow(dead_code)]

use wgpu::{BindGroupLayout, BindGroup};

use crate::{render_item_impl_default, rendering::{bind_groups::{storage, uniform}, wgpu::WGpu}, state::helper::render_item::RenderItem};

pub struct SingleBindingBindGroup
{
    pub layout: BindGroupLayout,
    pub bind_group: BindGroup
}

impl RenderItem for SingleBindingBindGroup
{
    render_item_impl_default!();
}

impl SingleBindingBindGroup
{
    pub fn bind_layout(wgpu: &mut WGpu, vertex: bool, fragment: bool, read_only: bool, storage: bool) -> BindGroupLayout
    {
        let entry = if storage
        {
            storage::storage_bind_group_layout_entry(0, vertex, fragment, read_only)
        }
        else
        {
            uniform::uniform_bind_group_layout_entry(0, vertex, fragment)
        };

        let bind_group_layout = wgpu.device().create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor
        {
            entries: &[entry],
            label: Some("single_binding_bind_group_layout"),
        });

        bind_group_layout
    }

    pub fn new(wgpu: &mut WGpu, name: &str, buffer: &wgpu::Buffer, vertex: bool, fragment: bool, read_only: bool, storage: bool) -> SingleBindingBindGroup
    {
        let bind_group_layout = Self::bind_layout(wgpu, vertex, fragment, read_only, storage);

        let bind_group_name = format!("{} single_binding_bind_group", name);
        let bind_group = wgpu.device().create_bind_group(&wgpu::BindGroupDescriptor
        {
            layout: &bind_group_layout,
            entries:
            &[
                wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() },
            ],
            label: Some(bind_group_name.as_str()),
        });

        SingleBindingBindGroup
        {
            layout: bind_group_layout,
            bind_group
        }
    }
}