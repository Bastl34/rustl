
// Due to uniforms requiring 16 byte (4 float) spacing, its needed to use pading
// position: [f32; 3]
// __padding: [f32; 1]
// --> 16
// https://sotrh.github.io/learn-wgpu/intermediate/tutorial10-lighting/#the-blinn-phong-model
// https://www.w3.org/TR/WGSL/#alignment-and-size

use nalgebra::{Vector3, Point3};

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightUniform
{
    position: [f32; 4],
    color: [f32; 4],
    intensity: f32,
    _padding: [f32; 3],
}


impl LightUniform
{
    pub fn new(position: Point3<f32>, color: Vector3<f32>, intensity: f32) -> Self
    {
        Self
        {
            position: [position.x, position.y, position.z, 1.0],
            color: [color.x, color.y, color.z, 1.0],
            intensity,
            _padding: [0.0, 0.0, 0.0]
        }
    }
}