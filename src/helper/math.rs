#![allow(dead_code)]

use std::f32::consts::PI;

use nalgebra::{Vector4, Vector3, Vector2};

pub fn approx_equal(a: f32, b: f32) -> bool
{
    let decimal_places = 6;

    let factor = 10.0f32.powi(decimal_places as i32);
    let a = (a * factor).trunc();
    let b = (b * factor).trunc();

    a == b
}

pub fn approx_equal_vec(a: Vector3<f32>, b: Vector3<f32>) -> bool
{
    approx_equal(a.x, b.x) && approx_equal(a.y, b.y) && approx_equal(a.z, b.z)
}

pub fn approx_zero(value: f32) -> bool
{
    let tolerance = 1e-6;
    value.abs() < tolerance
}

pub fn approx_zero_vec2(value: Vector2::<f32>) -> bool
{
    approx_zero(value.x) && approx_zero(value.y)
}

pub fn approx_zero_vec3(value: Vector3::<f32>) -> bool
{
    approx_zero(value.x) && approx_zero(value.y) && approx_zero(value.z)
}

pub fn approx_one_vec3(value: Vector3::<f32>) -> bool
{
    approx_equal_vec(value, Vector3::<f32>::new(1.0, 1.0, 1.0))
}

pub fn interpolate(a: f32, b: f32, f: f32) -> f32
{
    return a + f * (b - a);
}

pub fn interpolate_vec4(a: Vector4<f32>, b: Vector4<f32>, f: f32) -> Vector4<f32>
{
    Vector4::<f32>::new
    (
        interpolate(a.x, b.x, f),
        interpolate(a.y, b.y, f),
        interpolate(a.z, b.z, f),
        interpolate(a.w, b.w, f)
    )
}

pub fn yaw_pitch_from_direction(dir: Vector3::<f32>) -> (f32, f32)
{
    // https://github.com/bergerkiller/BKCommonLib/blob/master/src/main/java/com/bergerkiller/bukkit/common/utils/MathUtil.java

    let yaw;
    {
        // x and z are changed (because otherwise the result its not pointing in the right direction)
        let dx = dir.z;
        let dz = dir.x;

        yaw = dz.atan2(dx) - PI;
    }

    let pitch;
    {
        let dx = dir.x;
        let dy = -dir.y; // somewhow y is flipped
        let dz = dir.z;

        let dxz = ((dx * dx) + (dz * dz)).sqrt();

        pitch = -((dy / dxz)).atan();
    }

    (yaw, pitch)
}

/*
pub fn extract_rotation(matrix: Matrix4<f32>) -> Matrix3<f32>
{
    let submatrix = matrix.fixed_slice::<nalgebra::U3, nalgebra::U3>(0, 0);
    submatrix.into_owned()
}
*/