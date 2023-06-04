use nalgebra::{Point3, Matrix4};

use crate::{state::{helper::render_item::RenderItem}, render_item_impl_default};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform
{
    pub view_position: [f32; 4],
    pub view_proj: [[f32; 4]; 4],
}

impl RenderItem for CameraUniform
{
    render_item_impl_default!();
}

impl CameraUniform
{
    pub fn new() -> Self
    {
        Self
        {
            view_position: [0.0; 4],
            view_proj: nalgebra::Matrix4::<f32>::identity().into()
        }
    }

    pub fn update_view_proj(&mut self, pos:Point3::<f32>, projection: Matrix4<f32>, view: Matrix4<f32>)
    {
        self.view_position = pos.to_homogeneous().into();
        self.view_proj = (projection * view).into();
    }
}