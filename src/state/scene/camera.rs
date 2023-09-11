use std::mem::swap;

use nalgebra::{Matrix4, Perspective3, Point3, Isometry3, Vector3, Vector2};

use crate::{helper::{math::approx_equal, change_tracker::ChangeTracker}, state::helper::render_item::{RenderItemOption}, input::input_manager::InputManager};

use super::{node::NodeItem, camera_controller::{camera_controller::CameraControllerBox, fly_controller::FlyController}};

const DEFAULT_CAM_POS: Point3::<f32> = Point3::<f32>::new(0.0, 0.0, 0.0);
const DEFAULT_CAM_UP: Vector3::<f32> = Vector3::<f32>::new(0.0, 1.0, 0.0);
const DEFAULT_CAM_DIR: Vector3::<f32> = Vector3::<f32>::new(0.0, 0.0, -1.0);

//pub const OBLIQUE_CAM_POS: Vector3::<f32> = Vector3::<f32>::new(1.0, 0.0, 2.0);
pub const OBLIQUE_CAM_POS: Vector3::<f32> = Vector3::<f32>::new(-0.5, 0.5, 1.0);

pub const DEFAULT_FOVY: f32 = 90.0f32;

const DEFAULT_CLIPPING_NEAR: f32 = 0.001;
const DEFAULT_CLIPPING_FAR: f32 = 1000.0;

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

pub struct CameraData
{
    pub viewport_x: f32,    // 0.0-1.0
    pub viewport_y: f32, // 0.0-1.0
    pub viewport_width: f32, // 0.0-1.0
    pub viewport_height: f32, // 0.0-1.0

    pub resolution_aspect_ratio: f32,

    pub resolution_width: u32,
    pub resolution_height: u32,

    pub fovy: f32,

    pub eye_pos: Point3::<f32>,

    pub up: Vector3::<f32>,
    pub dir: Vector3::<f32>,

    pub clipping_near: f32,
    pub clipping_far: f32,

    pub projection: Perspective3<f32>,
    pub view: Matrix4<f32>,

    pub projection_inverse: Matrix4<f32>,
    pub view_inverse: Matrix4<f32>,
}

pub struct Camera
{
    pub id: u64,
    pub name: String,
    pub enabled: bool,

    pub data: ChangeTracker<CameraData>,

    pub controller: Option<CameraControllerBox>,
    pub node: Option<NodeItem>,

    pub render_item: RenderItemOption,
    pub bind_group_render_item: RenderItemOption,
}

impl Camera
{
    pub fn new(id: u64, name: String) -> Camera
    {
        Camera
        {
            id: id,
            name: name,
            enabled: true,

            data: ChangeTracker::new(CameraData
            {
                viewport_x: 0.0,
                viewport_y: 0.0,
                viewport_width: 1.0,
                viewport_height: 1.0,

                resolution_aspect_ratio: 0.0,

                resolution_width: 0,
                resolution_height: 0,

                fovy: DEFAULT_FOVY.to_radians(),

                eye_pos: DEFAULT_CAM_POS,

                up: DEFAULT_CAM_UP,
                dir: DEFAULT_CAM_DIR,

                clipping_near: DEFAULT_CLIPPING_NEAR,
                clipping_far: DEFAULT_CLIPPING_FAR,

                projection: Perspective3::<f32>::new(1.0f32, 0.0f32, DEFAULT_CLIPPING_NEAR, DEFAULT_CLIPPING_FAR),
                view: Matrix4::<f32>::identity(),

                projection_inverse: Matrix4::<f32>::identity(),
                view_inverse: Matrix4::<f32>::identity(),
            }),

            controller: None,
            node: None,

            render_item: None,
            bind_group_render_item: None
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

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<CameraData>
    {
        &mut self.data
    }

    pub fn init(&mut self, viewport_x: f32, viewport_y: f32, viewport_width: f32, viewport_height: f32, resolution_width: u32, resolution_height: u32)
    {
        let data = self.data.get_mut();

        data.viewport_x = viewport_x;
        data.viewport_y = viewport_y;
        data.viewport_width = viewport_width;
        data.viewport_height = viewport_height;

        data.resolution_width = resolution_width;
        data.resolution_height = resolution_height;

        data.resolution_aspect_ratio = resolution_width as f32 / resolution_height as f32;

        self.init_matrices();
    }

    pub fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, input_manager: &mut InputManager, frame_scale: f32) -> bool
    {
        let mut changed = false;
        let mut controller: Option<CameraControllerBox> = None;
        swap(&mut self.controller, &mut controller);

        let node = self.node.clone();
        let data = self.get_data_mut();

        if let Some(controller) = &mut controller
        {
            controller.update(node, scene, input_manager, data, frame_scale);

            // re-calculate matrices on if there was a change
            if self.data.changed()
            {
                self.init_matrices();
                changed = true;
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

    pub fn init_matrices(&mut self)
    {
        let data = self.data.get_mut();

        data.projection = Perspective3::new(data.resolution_aspect_ratio, data.fovy, data.clipping_near, data.clipping_far);

        //let target = Point3::<f32>::new(self.dir.x, self.dir.y, self.dir.z);
        let target = data.eye_pos + data.dir;

        data.view = Isometry3::look_at_rh(&data.eye_pos, &target, &data.up).to_homogeneous();

        data.projection_inverse = data.projection.inverse();
        data.view_inverse = data.view.try_inverse().unwrap();
    }

    pub fn add_controller_fly(&mut self, mouse_sensitivity: Vector2::<f32>, move_speed: f32, move_speed_shift: f32)
    {
        self.controller = Some(Box::new(FlyController::new(mouse_sensitivity, move_speed, move_speed_shift)));
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

        OPENGL_TO_WGPU_MATRIX * data.projection.to_homogeneous()
    }

    pub fn is_point_in_frustum(&self, point: &Point3<f32>) -> bool
    {
        let data = self.data.get_ref();

        let pv = data.projection.to_homogeneous() * data.view;
        let point_clip = pv * point.to_homogeneous();

        // Check if point is inside NDC space (Normalized Device Coordinates Space)
        point_clip.x.abs() <= point_clip.w && point_clip.y.abs() <= point_clip.w && point_clip.z.abs() <= point_clip.w
    }

    pub fn print(&self)
    {
        let data = self.data.get_ref();

        println!("name: {:?}", self.name);

        println!("id: {:?}", self.id);
        println!("name: {:?}", self.name);
        println!("enabled: {:?}", self.enabled);

        println!("viewport x: {:?}", data.viewport_x);
        println!("viewport y: {:?}", data.viewport_y);
        println!("viewport width: {:?}", data.viewport_width);
        println!("viewport height: {:?}", data.viewport_height);

        println!("resolution aspect_ratio: {:?}", data.resolution_aspect_ratio);

        println!("resolution width: {:?}", data.resolution_width);
        println!("resolution height: {:?}", data.resolution_height);

        println!("fov: {:?}", data.fovy);

        println!("eye_pos: {:?}", data.eye_pos);

        println!("up: {:?}", data.up);
        println!("dir: {:?}", data.dir);

        println!("clipping_near: {:?}", data.clipping_near);
        println!("clipping_far: {:?}", data.clipping_far);

        println!("projection: {:?}", data.projection);
        println!("view: {:?}", data.view);
    }

    pub fn print_short(&self)
    {
        let data = self.data.get_ref();

        println!(" - (CAMERA): id={} name={} enabled={} viewport=[x={}, y={}], [{}x{}], resolution={}x{}, fovy={} eye_pos={:?} near={}, far={}", self.id, self.name, self.enabled, data.viewport_x, data.viewport_y, data.viewport_width, data.viewport_height, data.resolution_width, data.resolution_height, data.fovy, data.eye_pos, data.clipping_near, data.clipping_far);
    }
}