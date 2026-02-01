#![allow(dead_code)]

use std::f32::consts::PI;

use nalgebra::{Matrix3, Matrix4, Point3, Rotation3, UnitQuaternion, Vector2, Vector3, Vector4};
use parry3d::query::Ray;

pub fn approx_equal(a: f32, b: f32) -> bool
{
    let decimal_places = 6;

    let factor = 10.0f32.powi(decimal_places as i32);
    let a = (a * factor).trunc();
    let b = (b * factor).trunc();

    a == b
}

pub fn approx_equal_with_decimal_places(a: f32, b: f32, decimal_places: i32) -> bool
{
    let factor = 10.0f32.powi(decimal_places);
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

pub fn approx_zero_vec4(value: &Vector4::<f32>) -> bool
{
    approx_zero(value.x) && approx_zero(value.y) && approx_zero(value.z) && approx_zero(value.w)
}

pub fn approx_one_vec3(value: &Vector3::<f32>) -> bool
{
    let one = Vector3::<f32>::new(1.0, 1.0, 1.0);
    approx_equal_vec(value, &one)
}

pub fn is_almost_integer(value: f32) -> bool
{
    let tolerance = 1e-6;
    (value - value.round()).abs() < tolerance
}

pub fn shortest_angle_dist(a: f32, b: f32) -> f32
{
    let mut diff = (b - a) % (2.0 * PI);
    if diff < -PI
    {
        diff += 2.0 * PI;
    }
    else if diff > PI
    {
        diff -= 2.0 * PI;
    }
    diff
}

pub fn interpolate(a: f32, b: f32, f: f32) -> f32
{
    return a + f * (b - a);
}

pub fn interpolate_angle(a: f32, b: f32, t: f32) -> f32
{
    let delta = shortest_angle_dist(a, b);
    a + delta * t
}

pub fn interpolate_vec3(a: &Vector3<f32>, b: &Vector3<f32>, f: f32) -> Vector3<f32>
{
    Vector3::<f32>::new
    (
        interpolate(a.x, b.x, f),
        interpolate(a.y, b.y, f),
        interpolate(a.z, b.z, f)
    )
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

pub fn interpolate_vec(a: &Vec<f32>, b: &Vec<f32>, f: f32) -> Vec<f32>
{
    let mut vec: Vec<f32> = Vec::with_capacity(a.len());
    vec.extend(vec![0.0; a.len()]);

    for i in 0..a.len()
    {
        vec[i] = interpolate(a[i], b[i], f);
    }

    vec
}

//https://github.com/dakom/awsm-renderer/blob/1c7df6b66a3507e11721d549d85c3cfeae146a1f/crate/src/animation/clip.rs#L151
pub fn cubic_spline_interpolate_vec3
(
    interpolation_time: f32,
    delta_time: f32,
    _prev_input_tangent: &Vector3::<f32>,
    prev_keyframe_value: &Vector3::<f32>,
    prev_output_tangent: &Vector3::<f32>,
    next_input_tangent: &Vector3::<f32>,
    next_keyframe_value: &Vector3::<f32>,
    _next_output_tangent: &Vector3::<f32>
) -> Vector3::<f32>
{
    let t = interpolation_time;
    let t2 = t * t;
    let t3 = t * t * t;

    let prev_tangent = delta_time * prev_output_tangent;
    let next_tangent = delta_time * next_input_tangent;

    ((2.0 * t3 - 3.0 * t2 + 1.0) * prev_keyframe_value)
    + ((t3 - 2.0 * t2 + t) * prev_tangent)
    + (( -2.0 * t3 + 3.0 * t2) * next_keyframe_value)
    + ((t3 - t2) * next_tangent)
}

pub fn cubic_spline_interpolate_vec4
(
    interpolation_time: f32,
    delta_time: f32,
    _prev_input_tangent: &Vector4::<f32>,
    prev_keyframe_value: &Vector4::<f32>,
    prev_output_tangent: &Vector4::<f32>,
    next_input_tangent: &Vector4::<f32>,
    next_keyframe_value: &Vector4::<f32>,
    _next_output_tangent: &Vector4::<f32>
) -> Vector4::<f32> {
    let t = interpolation_time;
    let t2 = t * t;
    let t3 = t * t * t;

    let prev_tangent = delta_time * prev_output_tangent;
    let next_tangent = delta_time * next_input_tangent;

    ((2.0 * t3 - 3.0 * t2 + 1.0) * prev_keyframe_value)
    + ((t3 - 2.0 * t2 + t) * prev_tangent)
    + (( -2.0 * t3 + 3.0 * t2) * next_keyframe_value)
    + ((t3 - t2) * next_tangent)
    //prev_keyframe_value.clone()
}

pub fn cubic_spline_interpolate_vec
(
    interpolation_time: f32,
    delta_time: f32,
    prev_input_tangent: &Vec::<f32>,
    prev_keyframe_value: &Vec::<f32>,
    prev_output_tangent: &Vec::<f32>,
    next_input_tangent: &Vec::<f32>,
    next_keyframe_value: &Vec::<f32>,
    _next_output_tangent: &Vec::<f32>
) -> Vec::<f32> {
    let t = interpolation_time;
    let t2 = t * t;
    let t3 = t * t * t;

    let mut vec: Vec<f32> = Vec::with_capacity(prev_input_tangent.len());
    vec.extend(vec![0.0; prev_input_tangent.len()]);

    for i in 0..prev_input_tangent.len()
    {
        let prev_tangent = delta_time * prev_output_tangent[i];
        let next_tangent = delta_time * next_input_tangent[i];

        vec[i] = ((2.0 * t3 - 3.0 * t2 + 1.0) * prev_keyframe_value[i])
        + ((t3 - 2.0 * t2 + t) * prev_tangent)
        + (( -2.0 * t3 + 3.0 * t2) * next_keyframe_value[i])
        + ((t3 - t2) * next_tangent)
        //prev_keyframe_value.clone()
    }

    vec
}

// https://github.com/BabylonJS/Babylon.js/blob/master/packages/dev/core/src/Maths/math.path.ts
pub fn bezier_interpolate(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32
{
    let f0 = 1.0 - 3.0 * x2 + 3.0 * x1;
    let f1 = 3.0 * x2 - 6.0 * x1;
    let f2 = 3.0 * x1;

    let mut refined_t = t;
    for _ in 0..5
    {
        let refined_t2 = refined_t * refined_t;
        let refined_t3 = refined_t2 * refined_t;

        let x = f0 * refined_t3 + f1 * refined_t2 + f2 * refined_t;
        let slope = 1.0 / (3.0 * f0 * refined_t2 + 2.0 * f1 * refined_t + f2);
        refined_t -= (x - t) * slope;
        refined_t = 1.0_f32.min(0.0_f32.max(refined_t));
    }

    3.0 * (1.0 - refined_t).powi(2) * refined_t * y1 + 3.0 * (1.0 - refined_t) * refined_t.powi(2) * y2 + refined_t.powi(3)
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
    let origin = Point3::new(ray.origin.x, ray.origin.y, ray.origin.z);
    let dir = Vector3::new(ray.dir.x, ray.dir.y, ray.dir.z);

    let ray_inverse_start = trans_inverse * origin.to_homogeneous();
    let ray_inverse_dir = trans_inverse * dir.to_homogeneous();

    Ray::new(Point3::from_homogeneous(ray_inverse_start).unwrap().into(), Vector3::from_homogeneous(ray_inverse_dir).unwrap().into())
}

/*
pub fn extract_rotation(matrix: Matrix4<f32>) -> Matrix3<f32>
{
    let submatrix = matrix.fixed_slice::<nalgebra::U3, nalgebra::U3>(0, 0);
    submatrix.into_owned()
}
*/

pub fn calculate_normal(v1: &Point3<f32>, v2: &Point3<f32>, v3: &Point3<f32>) -> Vector3<f32>
{
    let vec_1 = v2 - v1;
    let vec_2 = v3 - v1;

    let normal = vec_1.cross(&vec_2);
    normal.normalize()
}

pub fn snap_to_grid(value: f32, grid_size: f32) -> f32
{
    let lower_bound = (value / grid_size).floor() * grid_size;
    let upper_bound = (value / grid_size).ceil() * grid_size;

    let lower_distance = (value - lower_bound).abs();
    let upper_distance = (value - upper_bound).abs();

    if lower_distance < upper_distance
    {
        lower_bound
    }
    else
    {
        upper_bound
    }
}

pub fn snap_to_grid_vec2(value: Vector2<f32>, grid_size: f32) -> Vector2<f32>
{
    let mut vec = value.clone();
    vec.x = snap_to_grid(vec.x, grid_size);
    vec.y = snap_to_grid(vec.y, grid_size);

    vec
}

pub fn snap_to_grid_vec3(value: Vector3<f32>, grid_size: f32) -> Vector3<f32>
{
    let mut vec = value.clone();
    vec.x = snap_to_grid(vec.x, grid_size);
    vec.y = snap_to_grid(vec.y, grid_size);
    vec.z = snap_to_grid(vec.z, grid_size);

    vec
}

pub fn ray_plane_intersection(ray: &Ray, plane_normal: Vector3<f32>, plane_point: Point3<f32>) -> Option<Point3<f32>>
{
    let ray_dir = ray.dir.normalize();
    let ray_origin = ray.origin;

    let d = plane_normal.dot(&plane_point.coords);
    let denominator = plane_normal.dot(&ray_dir.into());

    // parallel
    if denominator.abs() < 1e-6
    {
        return None;
    }

    let t = (d - plane_normal.dot(&Vector3::<f32>::from(ray_origin))) / denominator;

    if !t.is_finite() || t < 0.0
    {
        return None;
    }

    Some((ray_origin + ray_dir * t).into())
}

pub fn signed_angle_between_points(origin: &Point3<f32>, p1: &Point3<f32>, p2: &Point3<f32>, reference_axis: &Vector3<f32>) -> f32
{
    let v1 = p1.coords - origin.coords;
    let v2 = p2.coords - origin.coords;

    let dot_product = v1.dot(&v2);
    let magnitude_product = v1.norm() * v2.norm();

    if magnitude_product == 0.0
    {
        return 0.0;
    }

    let cos_theta = (dot_product / magnitude_product).clamp(-1.0, 1.0);
    let angle = cos_theta.acos(); // always positive

    // get direction
    let cross_product = v1.cross(&v2);
    let direction = cross_product.dot(&reference_axis);

    if direction < 0.0
    {
        -angle
    }
    else
    {
        angle
    }
}

pub fn extract_rotation_only(matrix: &Matrix4<f32>) -> Matrix4<f32>
{
    // Extract the first 3 columns (x, y, z axes)
    let mut x = matrix.fixed_view::<3, 1>(0, 0).into_owned();
    let mut y = matrix.fixed_view::<3, 1>(0, 1).into_owned();
    let mut z = matrix.fixed_view::<3, 1>(0, 2).into_owned();

    // Remove scaling by normalizing the column vectors
    x.normalize_mut();
    y.normalize_mut();
    z.normalize_mut();

    // New matrix with only rotation, without scaling and without translation
    Matrix4::new
    (
        x.x, y.x, z.x, 0.0,
        x.y, y.y, z.y, 0.0,
        x.z, y.z, z.z, 0.0,
        0.0, 0.0, 0.0, 1.0,
    )
}

pub fn extract_rotation_as_euler_vec(matrix: &Matrix4<f32>) -> Vector3<f32>
{
    let rotation = extract_rotation_only(matrix);

    let sy = -rotation[(2, 0)];

    // normal calculation
    if sy.abs() < 1.0 - std::f32::EPSILON
    {
        let pitch = sy.asin();
        let roll = rotation[(2, 1)].atan2(rotation[(2, 2)]);
        let yaw = rotation[(1, 0)].atan2(rotation[(0, 0)]);
        Vector3::new(roll, pitch, yaw) // (X, Y, Z)
    }
    // gimbal lock
    else
    {
        let pitch = sy.asin();
        let roll = 0.0;
        let yaw = (-rotation[(0, 1)]).atan2(rotation[(1, 1)]);
        Vector3::new(roll, pitch, yaw)
    }
}

pub fn extract_rotation_quat_from_transform(transform: &Matrix4<f32>) -> UnitQuaternion<f32>
{
    let mut rot_mat = transform.fixed_view::<3, 3>(0, 0).clone_owned();

    // Normalize each column to remove scaling
    for i in 0..3
    {
        let col = rot_mat.column(i);
        let normalized = col.normalize();
        rot_mat.set_column(i, &normalized);
    }

    // Convert to quaternion
    UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(rot_mat))
}

pub fn look_at_rotation(target_dir: Vector3<f32>, up: Vector3<f32>) -> UnitQuaternion<f32>
{
    // In OpenGL/glTF, -Z is forward in local space
    // target_dir is the direction we want to look at (in world/parent space)
    // We need to build a rotation matrix where the local -Z axis points towards target_dir

    // The local -Z axis should point in the target_dir direction
    // So the Z column of the matrix should be -target_dir
    let forward = -target_dir.normalize();  // This will be the Z column
    let up_n = up.normalize();

    // Calculate right vector: right = up x forward
    let mut right = up_n.cross(&forward);
    if right.norm_squared() < 1e-6
    {
        // forward and up are ~ parallel -> use a "fallback"-axis
        // Use +Z as fallback to keep the character facing in the original direction
        let fallback = Vector3::new(0.0, 0.0, 1.0);
        right = up_n.cross(&fallback);

        // If that also fails (shouldn't happen), use X
        if right.norm_squared() < 1e-6
        {
            right = Vector3::x();
        }
    }
    let right = right.normalize();

    // Recalculate up to ensure orthogonality: up = forward x right
    let up_corrected = forward.cross(&right).normalize();

    // Build rotation matrix with columns [right, up, forward]
    // where forward = -target_dir (so the local -Z axis points towards target_dir)
    let rot_mat = Matrix3::from_columns(&[right, up_corrected, forward]);
    let rotation = Rotation3::from_matrix_unchecked(rot_mat);
    UnitQuaternion::from_rotation_matrix(&rotation)
}

pub fn extract_translation_from_transform(transform: &Matrix4<f32>) -> Vector3<f32>
{
    Vector3::new
    (
        transform[(0, 3)],
        transform[(1, 3)],
        transform[(2, 3)]
    )
}

pub fn extract_max_scale_from_transform(transform: &Matrix4<f32>) -> f32
{
    let scale = extract_scale_from_transform(transform);
    scale.x.max(scale.y).max(scale.z)
}

pub fn extract_scale_from_transform(transform: &Matrix4<f32>) -> Vector3<f32>
{
    let scale_x = transform.fixed_view::<3, 1>(0, 0).norm();
    let scale_y = transform.fixed_view::<3, 1>(0, 1).norm();
    let scale_z = transform.fixed_view::<3, 1>(0, 2).norm();

    Vector3::new(scale_x, scale_y, scale_z)
}