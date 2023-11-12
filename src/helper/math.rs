#![allow(dead_code)]

use std::f32::consts::PI;

use nalgebra::{Vector4, Vector3, Vector2, Matrix4, Point3};
use parry3d::query::Ray;

pub fn approx_equal(a: f32, b: f32) -> bool
{
    let decimal_places = 6;

    let factor = 10.0f32.powi(decimal_places as i32);
    let a = (a * factor).trunc();
    let b = (b * factor).trunc();

    a == b
}

pub fn approx_equal_vec(a: &Vector3<f32>, b: &Vector3<f32>) -> bool
{
    approx_equal(a.x, b.x) && approx_equal(a.y, b.y) && approx_equal(a.z, b.z)
}

pub fn approx_zero(value: f32) -> bool
{
    let tolerance = 1e-6;
    value.abs() < tolerance
}

pub fn approx_zero_vec2(value: &Vector2::<f32>) -> bool
{
    approx_zero(value.x) && approx_zero(value.y)
}

pub fn approx_zero_vec3(value: &Vector3::<f32>) -> bool
{
    approx_zero(value.x) && approx_zero(value.y) && approx_zero(value.z)
}

pub fn approx_one_vec3(value: &Vector3::<f32>) -> bool
{
    let one = Vector3::<f32>::new(1.0, 1.0, 1.0);
    approx_equal_vec(value, &one)
}

pub fn interpolate(a: f32, b: f32, f: f32) -> f32
{
    return a + f * (b - a);
}

pub fn interpolate_vec4(a: &Vector4<f32>, b: &Vector4<f32>, f: f32) -> Vector4<f32>
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
    let pitch = dir.y.asin();
    let yaw = dir.x.atan2(dir.z);

    (yaw, pitch)
}

pub fn yaw_pitch_to_direction(yaw: f32, pitch: f32) -> Vector3::<f32>
{
    Vector3::<f32>::new
    (
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos()
    )
}

pub fn inverse_ray(ray: &Ray, trans_inverse: &Matrix4<f32>) -> Ray
{
    let ray_inverse_start = trans_inverse * ray.origin.to_homogeneous();
    let ray_inverse_dir = trans_inverse * ray.dir.to_homogeneous();

    Ray::new(Point3::from_homogeneous(ray_inverse_start).unwrap(), Vector3::from_homogeneous(ray_inverse_dir).unwrap())
}

/*
pub fn extract_rotation(matrix: Matrix4<f32>) -> Matrix3<f32>
{
    let submatrix = matrix.fixed_slice::<nalgebra::U3, nalgebra::U3>(0, 0);
    submatrix.into_owned()
}
*/