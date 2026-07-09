#![allow(dead_code)]

use std::{f32::consts::PI, mem::swap};

use nalgebra::{Isometry3, Matrix4, Orthographic3, Perspective3, Point2, Point3, Vector2, Vector3, Vector4};
use parry3d::query::Ray;
use serde::{Deserialize, Serialize, Serializer};

use crate::{console_log, helper::{change_tracker::ChangeTracker, math::{approx_equal, approx_zero}, option_or_id::OptionOrId}, state::{helper::render_item::RenderItemOption, scene::{camera_controller::pan_controller::PanController, utilities::tags::Tags}, state::InputOutput}};

use super::{camera_controller::{camera_controller::CameraControllerBox, fly_controller::FlyController, target_rotation_controller::TargetRotationController}, layers::LAYER_MASK_ALL, manager::id_manager, node::NodeItem};

use crate::state::scene::exporter::serialization_helper;

const DEFAULT_CAM_POS: Point3::<f32> = Point3::<f32>::new(0.0, 0.0, 0.0);
const DEFAULT_CAM_UP: Vector3::<f32> = Vector3::<f32>::new(0.0, 1.0, 0.0);
const DEFAULT_CAM_DIR: Vector3::<f32> = Vector3::<f32>::new(0.0, 0.0, -1.0);

//pub const OBLIQUE_CAM_POS: Vector3::<f32> = Vector3::<f32>::new(1.0, 0.0, 2.0);
//pub const OBLIQUE_CAM_POS: Vector3::<f32> = Vector3::<f32>::new(-0.5, 0.5, 1.0);

const DEFAULT_LEFT_EAR_POS: Point3<f32> = Point3::<f32>::new(-1.0, 0.0, 0.0);
const DEFAULT_RIGHT_EAR_POS: Point3<f32> = Point3::<f32>::new(1.0, 0.0, 0.0);

pub const DEFAULT_FOVY: f32 = 90.0f32;

const DEFAULT_CLIPPING_NEAR: f32 = 0.1;
pub const DEFAULT_CLIPPING_FAR: f32 = 1000.0;

const FRUSTUM_CULLING_EPSILON: f32 = 0.0001;

/*
pub const OPENGL_TO_WGPU_MATRIX: nalgebra::Matrix4<f32> = nalgebra::Matrix4::new
(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.0,
    0.0, 0.0, 0.5, 1.0,
);
*/


pub const OPENGL_TO_WGPU_MATRIX: nalgebra::Matrix4<f32> = nalgebra::Matrix4::new
(
    1.0, 0.0, 0.0, 0.0,
    0.0, 1.0, 0.0, 0.0,
    0.0, 0.0, 0.5, 0.5,
    0.0, 0.0, 0.0, 1.0,
);

pub type CameraItem = Box<Camera>;

#[derive(PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum CameraProjectionType
{
    Perspective,
    Orthogonal
}

#[derive(Debug, Clone, Copy)]
pub struct FrustumPlanes
{
    pub left: Vector4<f32>,
    pub right: Vector4<f32>,
    pub bottom: Vector4<f32>,
    pub top: Vector4<f32>,
    pub near: Vector4<f32>,
    pub far: Vector4<f32>,
}

impl Default for FrustumPlanes
{
    fn default() -> Self
    {
        Self
        {
            left: Vector4::zeros(),
            right: Vector4::zeros(),
            bottom: Vector4::zeros(),
            top: Vector4::zeros(),
            near: Vector4::zeros(),
            far: Vector4::zeros(),
        }
    }
}

impl FrustumPlanes
{
    pub fn is_sphere_visible(&self, center: &Point3<f32>, radius: f32) -> bool
    {
        let planes =
        [
            &self.left,
            &self.right,
            &self.bottom,
            &self.top,
            &self.near,
            &self.far
        ];

        for plane in &planes
        {
            let distance = plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w;

            if distance + FRUSTUM_CULLING_EPSILON < -radius
            {
                return false;
            }
        }

        true
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Viewport
{
    pub x: f32,      // 0.0-1.0
    pub y: f32,      // 0.0-1.0
    pub width: f32,  // 0.0-1.0
    pub height: f32, // 0.0-1.0
}

#[derive(Serialize, Deserialize)]
pub struct CameraData
{
    viewport: Viewport,

    pub resolution_aspect_ratio: f32,

    pub resolution_width: u32,
    pub resolution_height: u32,

    pub fovy: f32,

    pub eye_pos: Point3::<f32>,
    pub left_ear_pos: Point3::<f32>,
    pub right_ear_pos: Point3::<f32>,

    pub use_target_node_for_ears: bool,

    pub up: Vector3::<f32>,
    pub dir: Vector3::<f32>,

    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,

    pub clipping_near: f32,
    pub clipping_far: f32,

    pub projection_type: CameraProjectionType,

    pub culling_mask: u32, // bitmask, matched against node layer_mask

    #[serde(skip)]
    pub projection: Matrix4<f32>,
    #[serde(skip)]
    pub projection_inverse: Matrix4<f32>,

    #[serde(skip)]
    pub view: Matrix4<f32>,
    #[serde(skip)]
    pub view_inverse: Matrix4<f32>,

    #[serde(skip)]
    pub frustum_planes: FrustumPlanes,
}

impl CameraData
{
    pub fn get_viewport(&self) -> Viewport
    {
        self.viewport.clone()
    }

    // camera viewport in surface pixels (top-left origin): [x, y, width, height]
    // rounded to whole pixels via the viewport edges so that rasterization (set_viewport),
    // the ssao pixel clamp and adjacent camera viewports agree on pixel ownership
    // (fractional viewport splits would otherwise bleed one pixel row/column between cameras)
    pub fn viewport_px(&self) -> [f32; 4]
    {
        let res_width = self.resolution_width as f32;
        let res_height = self.resolution_height as f32;

        let x0 = (self.viewport.x * res_width).round();
        let x1 = ((self.viewport.x + self.viewport.width) * res_width).round();

        // set_viewport uses top-left origin (the viewport values use bottom-left origin)
        let y0 = ((1.0 - self.viewport.y - self.viewport.height) * res_height).round();
        let y1 = ((1.0 - self.viewport.y) * res_height).round();

        [x0, y0, x1 - x0, y1 - y0]
    }
}


fn serialize_controller<S>(controller: &Option<CameraControllerBox>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{

    if let Some(controller) = controller
    {
        if controller.is_serializable()
        {
            controller.serialize(serializer)
        }
        else
        {
            Err(serde::ser::Error::custom(format!("CameraController '{}' is not serializable", controller.get_base().name)))
        }
    }
    else
    {
        serializer.serialize_none()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Camera
{
    pub id: u32,
    pub uuid: String,

    pub name: String,
    pub enabled: bool,

    pub data: ChangeTracker<CameraData>,

    pub tags: Tags,

    #[serde(serialize_with = "serialize_controller")]
    pub controller: Option<CameraControllerBox>,

    #[serde(serialize_with = "serialization_helper::serialize_node", deserialize_with = "serialization_helper::deserialize_node")]
    pub node: OptionOrId<NodeItem>,

    #[serde(skip, default)]
    pub render_item: RenderItemOption,

    #[serde(skip, default)]
    pub bind_group_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub hzb_texture_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub hzb_downsample_bind_group_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub visibility_buffer_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub hzb_occlusion_bind_group_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub depth_export_bind_group_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub ssao_bind_group_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub indirect_args_render_item: RenderItemOption,

    #[serde(skip, default)]
    pub visible_nodes_last_frame: Vec<u32>,
}

impl Default for Camera
{
    fn default() -> Self
    {
        Camera::new("Default Camera".to_string())
    }
}

impl Camera
{
    pub fn new(name: String) -> Camera
    {
        Camera
        {
            id: id_manager::get_next_camera_id(),
            uuid: uuid::Uuid::new_v4().to_string(),

            name: name,
            enabled: true,

            data: ChangeTracker::new(CameraData
            {

                viewport: Viewport
                {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                resolution_aspect_ratio: 1.0,

                resolution_width: 0,
                resolution_height: 0,

                fovy: DEFAULT_FOVY.to_radians(),

                eye_pos: DEFAULT_CAM_POS,
                left_ear_pos: DEFAULT_LEFT_EAR_POS,
                right_ear_pos: DEFAULT_RIGHT_EAR_POS,

                use_target_node_for_ears: true,

                up: DEFAULT_CAM_UP,
                dir: DEFAULT_CAM_DIR,

                left: -1.0,
                right: 1.0,
                top: 1.0,
                bottom: -1.0,

                clipping_near: DEFAULT_CLIPPING_NEAR,
                clipping_far: DEFAULT_CLIPPING_FAR,

                projection_type: CameraProjectionType::Perspective,

                culling_mask: LAYER_MASK_ALL,

                projection: Perspective3::<f32>::new(1.0f32, 0.0f32, DEFAULT_CLIPPING_NEAR, DEFAULT_CLIPPING_FAR).to_homogeneous(),
                projection_inverse: Matrix4::<f32>::identity(),

                view: Matrix4::<f32>::identity(),
                view_inverse: Matrix4::<f32>::identity(),

                frustum_planes: FrustumPlanes::default(),
            }),

            tags: Tags::new(),

            controller: None,
            node: OptionOrId::None,

            render_item: None,
            bind_group_render_item: None,
            hzb_texture_render_item: None,
            hzb_downsample_bind_group_render_item: None,
            visibility_buffer_render_item: None,
            hzb_occlusion_bind_group_render_item: None,
            depth_export_bind_group_render_item: None,
            ssao_bind_group_render_item: None,
            indirect_args_render_item: None,

            visible_nodes_last_frame: Vec::new(),
        }
    }

    pub fn get_data(&self) -> &CameraData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<CameraData>
    {
        &self.data
    }

    pub fn set_node(&mut self, node: NodeItem)
    {
        self.node = OptionOrId::Some(node);
    }

    pub fn remove_node(&mut self)
    {
        self.node = OptionOrId::None;
    }

    pub fn get_forward(&self) -> Vector3<f32>
    {
        self.get_data().dir
    }

    pub fn get_up(&self) -> Vector3<f32>
    {
        self.get_data().up
    }

    pub fn get_right(&self) -> Vector3<f32>
    {
        self.get_forward().cross(&self.get_up())
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<CameraData>
    {
        &mut self.data
    }

    pub fn init(&mut self, viewport_x: f32, viewport_y: f32, viewport_width: f32, viewport_height: f32, resolution_width: u32, resolution_height: u32)
    {
        let data = self.data.get_mut();

        data.viewport.x = viewport_x;
        data.viewport.y = viewport_y;
        data.viewport.width = viewport_width;
        data.viewport.height = viewport_height;

        data.resolution_width = resolution_width;
        data.resolution_height = resolution_height;

        data.resolution_aspect_ratio = resolution_width as f32 / resolution_height as f32;

        self.init_matrices();
    }

    pub fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, io: &mut InputOutput, frame_scale: f32) -> bool
    {
        if !self.enabled
        {
            return false;
        }

        let mut changed = false;
        let mut controller: Option<CameraControllerBox> = None;
        swap(&mut self.controller, &mut controller);

        if let Some(controller) = &mut controller
        {
            if controller.get_base().is_enabled
            {
                let node = self.node.as_ref().cloned();
                let data = self.get_data_mut();

                let processed = controller.update(node, scene, io, data, frame_scale);

                // re-calculate matrices on if there was a change
                if processed
                {
                    self.init_matrices();
                    changed = true;
                }
            }
        }

        swap(&mut controller, &mut self.controller);

        changed
    }

    pub fn update_resolution(&mut self, resolution_width: u32, resolution_height: u32)
    {
        let data = self.data.get_mut();

        data.resolution_width = resolution_width;
        data.resolution_height = resolution_height;

        data.resolution_aspect_ratio = resolution_width as f32 / resolution_height as f32;
    }

    pub fn update_viewport(&mut self, viewport_x: f32, viewport_y: f32, viewport_width: f32, viewport_height: f32)
    {
        let data = self.data.get_mut();

        data.viewport.x = viewport_x;
        data.viewport.y = viewport_y;
        data.viewport.width = viewport_width;
        data.viewport.height = viewport_height;
    }

    pub fn init_matrices(&mut self)
    {
        let data = self.data.get_mut();

        let viewport_w_px = (data.viewport.width  * data.resolution_width  as f32).max(1.0);
        let viewport_h_px = (data.viewport.height * data.resolution_height as f32).max(1.0);
        let viewport_aspect = viewport_w_px / viewport_h_px;

        if data.projection_type == CameraProjectionType::Perspective
        {
            data.projection = Perspective3::new(viewport_aspect, data.fovy, data.clipping_near, data.clipping_far).to_homogeneous();
        }
        else
        {
            // keep vertical extent (top/bottom) as authored, scale horizontal to viewport aspect
            let half_h = (data.top - data.bottom) * 0.5;
            let center_y = (data.top + data.bottom) * 0.5;
            let half_w = half_h * viewport_aspect;
            let center_x = (data.left + data.right) * 0.5;

            let left   = center_x - half_w;
            let right  = center_x + half_w;
            let bottom = center_y - half_h;
            let top    = center_y + half_h;

            data.projection = Orthographic3::new(left, right, bottom, top, data.clipping_near, data.clipping_far).to_homogeneous();
        }

        let target = data.eye_pos + data.dir;

        data.view = Isometry3::look_at_rh(&data.eye_pos, &target, &data.up).to_homogeneous();

        data.projection_inverse = data.projection.try_inverse().unwrap();
        data.view_inverse = data.view.try_inverse().unwrap();

        self.update_frustum_planes();
    }

    fn update_frustum_planes(&mut self)
    {
        let data = self.data.get_mut();
        let view_projection = data.projection * data.view;

        // Each plane is represented as a Vector4 (a, b, c, d) where ax + by + cz + d = 0

        // Left plane: row4 + row1
        let left = (view_projection.row(3) + view_projection.row(0)).transpose();

        // Right plane: row4 - row1
        let right = (view_projection.row(3) - view_projection.row(0)).transpose();

        // Bottom plane: row4 + row2
        let bottom = (view_projection.row(3) + view_projection.row(1)).transpose();

        // Top plane: row4 - row2
        let top = (view_projection.row(3) - view_projection.row(1)).transpose();

        // Near plane: row4 + row3
        let near = (view_projection.row(3) + view_projection.row(2)).transpose();

        // Far plane: row4 - row3
        let far = (view_projection.row(3) - view_projection.row(2)).transpose();

        let normalize_plane = |plane: Vector4<f32>| -> Vector4<f32>
        {
            let normal = Vector3::new(plane.x, plane.y, plane.z);
            let length = normal.norm();

            if length > 1e-6
            {
                plane / length
            }
            else
            {
                plane
            }
        };

        data.frustum_planes = FrustumPlanes
        {
            left: normalize_plane(left),
            right: normalize_plane(right),
            bottom: normalize_plane(bottom),
            top: normalize_plane(top),
            near: normalize_plane(near),
            far: normalize_plane(far),
        };
    }

    pub fn add_controller_fly(&mut self, collision: bool, mouse_sensitivity: Vector2::<f32>, move_speed: f32, move_speed_shift: f32, viewport_only: bool)
    {
        self.controller = Some(Box::new(FlyController::new(collision, mouse_sensitivity, move_speed, move_speed_shift, viewport_only)));
    }

    pub fn add_controller_pan(&mut self, mouse_wheel_sensitivity: f32, move_speed: f32, move_speed_shift: f32, viewport_only: bool)
    {
        self.controller = Some(Box::new(PanController::new(mouse_wheel_sensitivity, move_speed, move_speed_shift, viewport_only)));
    }

    pub fn add_controller_target_rotation(&mut self, radius: f32, mouse_sensitivity: Vector2::<f32>, mouse_wheel_sensitivity: f32)
    {
        self.controller = Some(Box::new(TargetRotationController::new(radius, 0.0, PI / 8.0, mouse_sensitivity, mouse_wheel_sensitivity)));
    }

    pub fn remove_controller(&mut self)
    {
        self.controller = None;
    }

    pub fn is_default_cam(&self) -> bool
    {
        let data = self.data.get_ref();

        (
            approx_equal(data.eye_pos.x, DEFAULT_CAM_POS.x)
            &&
            approx_equal(data.eye_pos.y, DEFAULT_CAM_POS.y)
            &&
            approx_equal(data.eye_pos.z, DEFAULT_CAM_POS.z)
        )
        &&
        (
            approx_equal(data.dir.x, DEFAULT_CAM_DIR.x)
            &&
            approx_equal(data.dir.y, DEFAULT_CAM_DIR.y)
            &&
            approx_equal(data.dir.z, DEFAULT_CAM_DIR.z)
        )
        &&
        (
            approx_equal(data.up.x, DEFAULT_CAM_UP.x)
            &&
            approx_equal(data.up.y, DEFAULT_CAM_UP.y)
            &&
            approx_equal(data.up.z, DEFAULT_CAM_UP.z)
        )
        &&
        approx_equal(data.fovy, DEFAULT_FOVY.to_radians())
        &&
        approx_equal(data.clipping_near, DEFAULT_CLIPPING_NEAR)
        &&
        approx_equal(data.clipping_far, DEFAULT_CLIPPING_FAR)
    }

    pub fn set_cam_position(&mut self, eye_pos: Point3::<f32>, dir: Vector3::<f32>)
    {
        let data = self.data.get_mut();

        data.eye_pos = eye_pos;
        data.dir = dir;

        self.init_matrices();
    }

    pub fn webgpu_projection(&self) -> nalgebra::Matrix4<f32>
    {
        let data = self.data.get_ref();

        OPENGL_TO_WGPU_MATRIX * data.projection
    }

    pub fn is_point_in_frustum(&self, point: &Point3<f32>) -> bool
    {
        let data = self.data.get_ref();

        let pv = data.projection * data.view;
        let point_clip = pv * point.to_homogeneous();

        // Check if point is inside NDC space (Normalized Device Coordinates Space)
        point_clip.x.abs() <= point_clip.w && point_clip.y.abs() <= point_clip.w && point_clip.z.abs() <= point_clip.w
    }

    pub fn is_sphere_in_frustum(&self, center: &Point3<f32>, radius: f32) -> bool
    {
        let data = self.data.get_ref();
        let result = data.frustum_planes.is_sphere_visible(center, radius);
        result
    }

    pub fn is_point_in_viewport(&self, point: &Point2<f32>) -> bool
    {
        Self::is_point_in_viewport_data(self.get_data(), point)
    }

    pub fn is_point_in_viewport_data(data: &CameraData, point: &Point2<f32>) -> bool
    {
        let x0 = data.viewport.x * data.resolution_width as f32;
        let y0 = data.viewport.y * data.resolution_height as f32;

        let width = data.viewport.width * data.resolution_width as f32;
        let height = data.viewport.height * data.resolution_height as f32;

        let x1 = x0 + width;
        let y1 = y0 + height;

        if point.x >= x0 && point.x < x1
        {
            if point.y >= y0 && point.y < y1
            {
                return true;
            }
        }

        false
    }

    pub fn screen_to_world(&self, point: &Point2<f32>) -> Vector3<f32>
    {
        let data = self.get_data();

        Self::screen_to_world_data(data, point)
    }

    pub fn screen_to_world_data(data: &CameraData, point: &Point2<f32>) -> Vector3<f32>
    {
        let x_f = point.x as f32 - (data.viewport.x * data.resolution_width as f32);
        let y_f = point.y as f32 - (data.viewport.y * data.resolution_height as f32);

        let w = data.viewport.width as f32 * data.resolution_width as f32;
        let h = data.viewport.height as f32 * data.resolution_height as f32;

        //map x/y to -1 <=> +1
        let sensor_x = ((x_f + 0.5) / w) * 2.0 - 1.0;
        let sensor_y = ((y_f + 0.5) / h) * 2.0 - 1.0;

        let normalized_pos = Vector4::<f32>::new(sensor_x, sensor_y, 0.0, 1.0);

        let mut camera_space = data.projection_inverse * normalized_pos;
        camera_space /= camera_space.w;

        let world_space = data.view_inverse * camera_space;

        world_space.xyz()
    }

    pub fn get_ray_from_viewport_coordinates(&self, point: &Point2<f32>) -> Ray
    {
        let data = self.get_data();

        let x_f = point.x as f32 - (data.viewport.x * data.resolution_width as f32);
        let y_f = point.y as f32 - (data.viewport.y * data.resolution_height as f32);

        let w = data.viewport.width as f32 * data.resolution_width as f32;
        let h = data.viewport.height as f32 * data.resolution_height as f32;

        //map x/y to -1 <=> +1
        let sensor_x = ((x_f + 0.5) / w) * 2.0 - 1.0;
        let sensor_y = ((y_f + 0.5) / h) * 2.0 - 1.0;

        let clip_point_near = Point3::new(sensor_x, sensor_y, -1.0);
        let clip_point_far = Point3::new(sensor_x, sensor_y, 1.0);

        let unprojected_near = data.projection_inverse.transform_point(&clip_point_near);
        let unprojected_far = data.projection_inverse.transform_point(&clip_point_far);

        let near_point = data.view_inverse.transform_point(&unprojected_near);
        let far_point = data.view_inverse.transform_point(&unprojected_far);

        let ray_dir = (far_point - near_point).normalize();

        let mut ray = Ray::new(near_point.into(), parry3d::math::Vec3::new(ray_dir.x, ray_dir.y, ray_dir.z));
        ray.dir = ray.dir.normalize();

        ray
    }

    pub fn get_viewport_coordinates_from_point(&self, point: &Point3<f32>) -> Point2<f32>
    {
        let data = self.get_data();

        let w = data.viewport.width as f32 * data.resolution_width as f32;
        let h = data.viewport.height as f32 * data.resolution_height as f32;

        let camera_point = data.view.transform_point(&point);
        let clip_space_point = data.projection.transform_point(&camera_point);

        let screen_x = ((clip_space_point.x + 1.0) * 0.5 * w as f32) as f32 + (data.viewport.x * data.resolution_width as f32);
        let screen_y = ((clip_space_point.y + 1.0) * 0.5 * h as f32) as f32 + (data.viewport.y * data.resolution_height as f32);

        // reduce by 0.5 because the point was the center of the pixel
        Point2::new(screen_x - 0.5, screen_y - 0.5)
    }

    pub fn get_viewport_width_in_px(&self) -> u32
    {
        let data = self.get_data();
        (data.viewport.width * data.resolution_width as f32).ceil() as u32
    }

    pub fn get_viewport_height_in_px(&self) -> u32
    {
        let data = self.get_data();
        (data.viewport.height * data.resolution_height as f32).ceil() as u32
    }

    pub fn get_left_right_ear_positions(&self) -> (Point3::<f32>, Point3<f32>)
    {
        let left = self.get_data().left_ear_pos;
        let right = self.get_data().right_ear_pos;

        if self.get_data().use_target_node_for_ears && self.node.is_some()
        {
            let node = self.node.as_ref().unwrap();
            let node = node.read().unwrap();
            let transform = node.get_full_transform();

            let left = transform.transform_point(&left);
            let right = transform.transform_point(&right);
            (left, right)
        }
        else
        {
            let left = self.get_data().view_inverse.transform_point(&left);
            let right = self.get_data().view_inverse.transform_point(&right);
            (left, right)
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        let mut viewport_x;
        let mut viewport_y;
        let mut viewport_width;
        let mut viewport_height;

        let mut fovy;

        let mut eye_pos;
        let mut left_ear_pos;
        let mut right_ear_pos;

        let mut use_target_node_for_ears;

        let mut up;
        let mut dir;

        let mut left;
        let mut right;
        let mut top;
        let mut bottom;

        let mut clipping_near;
        let mut clipping_far;

        let mut projection_type;

        {
            let data = self.data.get_ref();

            viewport_x = data.viewport.x;
            viewport_y = data.viewport.y;
            viewport_width = data.viewport.width;
            viewport_height = data.viewport.height;

            fovy = data.fovy.to_degrees();

            eye_pos = data.eye_pos;
            left_ear_pos = data.left_ear_pos;
            right_ear_pos = data.right_ear_pos;

            use_target_node_for_ears = data.use_target_node_for_ears;

            up = data.up;
            dir = data.dir;

            left = data.left;
            right = data.right;
            top = data.top;
            bottom = data.bottom;

            clipping_near = data.clipping_near;
            clipping_far = data.clipping_far;

            projection_type = data.projection_type;
        }

        let mut changed = false;

        ui.horizontal(|ui|
        {
            ui.label("Projection:");
            changed = ui.radio_value(&mut projection_type, CameraProjectionType::Perspective, "Perspective").changed() || changed;
            changed = ui.radio_value(&mut projection_type, CameraProjectionType::Orthogonal, "Orthogonal").changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Viewport Offset:");
            changed = ui.add(egui::DragValue::new(&mut viewport_x).range(0.0..=1.0).speed(0.01).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut viewport_y).range(0.0..=1.0).speed(0.01).prefix("y: ")).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Viewport Size:");
            changed = ui.add(egui::DragValue::new(&mut viewport_width).range(0.0..=1.0).speed(0.01).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut viewport_height).range(0.0..=1.0).speed(0.01).prefix("y: ")).changed() || changed;

            // prevent empty viewport
            if approx_zero(viewport_width)
            {
                viewport_width = 0.001;
            }

            if approx_zero(viewport_height)
            {
                viewport_height = 0.001;
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Position:");
            changed = ui.add(egui::DragValue::new(&mut eye_pos.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut eye_pos.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut eye_pos.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Left ear position:");
            changed = ui.add(egui::DragValue::new(&mut left_ear_pos.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut left_ear_pos.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut left_ear_pos.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Right ear position:");
            changed = ui.add(egui::DragValue::new(&mut right_ear_pos.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut right_ear_pos.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut right_ear_pos.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        changed = ui.checkbox(&mut use_target_node_for_ears, "use use target node for ears").changed() || changed;

        ui.horizontal(|ui|
        {
            ui.label("Direction Vector:");
            changed = ui.add(egui::DragValue::new(&mut dir.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut dir.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut dir.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Up Vector:");
            changed = ui.add(egui::DragValue::new(&mut up.x).speed(0.1).prefix("x: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut up.y).speed(0.1).prefix("y: ")).changed() || changed;
            changed = ui.add(egui::DragValue::new(&mut up.z).speed(0.1).prefix("z: ")).changed() || changed;
        });

        if self.get_data().projection_type == CameraProjectionType::Perspective
        {
            changed = ui.add(egui::Slider::new(&mut fovy, 0.001..=180.0).suffix(" °").text("Field of view (fov)")).changed() || changed;
        }
        else
        {
            ui.horizontal(|ui|
            {
                changed = ui.add(egui::DragValue::new(&mut left).speed(0.01).prefix("left: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut right).speed(0.01).prefix("right: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut top).speed(0.01).prefix("top: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut bottom).speed(0.01).prefix("bottom: ")).changed() || changed;
            });
        }

        changed = ui.add(egui::Slider::new(&mut clipping_near, 0.001..=1000.0).text("Near clipping plane")).changed() || changed;
        changed = ui.add(egui::Slider::new(&mut clipping_far, 1.0..=100000.0).text("Far clipping plane")).changed() || changed;

        if changed
        {
            let data = self.get_data_mut().get_mut();

            data.viewport.x = viewport_x;
            data.viewport.y = viewport_y;
            data.viewport.width = viewport_width;
            data.viewport.height = viewport_height;
            data.fovy = fovy.to_radians();

            data.eye_pos = eye_pos;
            data.left_ear_pos = left_ear_pos;
            data.right_ear_pos = right_ear_pos;

            data.use_target_node_for_ears = use_target_node_for_ears;

            data.up = up;
            data.dir = dir;

            data.left = left;
            data.right = right;
            data.top = top;
            data.bottom = bottom;

            data.clipping_near = clipping_near;
            data.clipping_far = clipping_far;

            if data.clipping_near >= data.clipping_far
            {
                data.clipping_near = data.clipping_far - 0.001
            }

            data.projection_type = projection_type;

            self.init_matrices();
        }
    }

    pub fn print(&self)
    {
        let data = self.data.get_ref();

        console_log!("name: {:?}", self.name);

        console_log!("id: {:?}", self.id);
        console_log!("name: {:?}", self.name);
        console_log!("enabled: {:?}", self.enabled);

        console_log!("viewport x: {:?}", data.viewport.x);
        console_log!("viewport y: {:?}", data.viewport.y);
        console_log!("viewport width: {:?}", data.viewport.width);
        console_log!("viewport height: {:?}", data.viewport.height);

        console_log!("resolution aspect_ratio: {:?}", data.resolution_aspect_ratio);

        console_log!("resolution width: {:?}", data.resolution_width);
        console_log!("resolution height: {:?}", data.resolution_height);

        console_log!("fov: {:?}", data.fovy);

        console_log!("eye_pos: {:?}", data.eye_pos);

        console_log!("up: {:?}", data.up);
        console_log!("dir: {:?}", data.dir);

        console_log!("clipping_near: {:?}", data.clipping_near);
        console_log!("clipping_far: {:?}", data.clipping_far);

        console_log!("projection: {:?}", data.projection);
        console_log!("view: {:?}", data.view);
    }

    pub fn print_short(&self)
    {
        let data = self.data.get_ref();

        console_log!(" - (CAMERA): id={} name={} enabled={} viewport=[x={}, y={}], [{}x{}], resolution={}x{}, fovy={} eye_pos={:?} near={}, far={}", self.id, self.name, self.enabled, data.viewport.x, data.viewport.y, data.viewport.width, data.viewport.height, data.resolution_width, data.resolution_height, data.fovy, data.eye_pos, data.clipping_near, data.clipping_far);
    }
}