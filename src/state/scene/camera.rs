use nalgebra::{Matrix4, Perspective3, Point3, Isometry3, Vector3};

use crate::{helper::math::approx_equal, state::helper::render_item::{RenderItemOption}};

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

pub struct Camera
{
    pub id: u64,
    pub name: String,
    pub enabled: bool,

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

            viewport_x: 0.0,
            viewport_y: 0.0,
            viewport_width: 0.0,
            viewport_height: 0.0,

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

            render_item: None,
            bind_group_render_item: None
        }
    }

    pub fn init(&mut self, viewport_x: f32, viewport_y: f32, viewport_width: f32, viewport_height: f32, resolution_width: u32, resolution_height: u32)
    {
        self.viewport_x = viewport_x;
        self.viewport_y = viewport_y;
        self.viewport_width = viewport_width;
        self.viewport_height = viewport_height;

        self.resolution_width = resolution_width;
        self.resolution_height = resolution_height;

        self.resolution_aspect_ratio = resolution_width as f32 / resolution_height as f32;

        self.init_matrices();
    }

    pub fn update_resolution(&mut self, resolution_width: u32, resolution_height: u32)
    {
        self.resolution_width = resolution_width;
        self.resolution_height = resolution_height;

        self.resolution_aspect_ratio = resolution_width as f32 / resolution_height as f32;
    }

    pub fn init_matrices(&mut self)
    {
        self.projection = Perspective3::new(self.resolution_aspect_ratio, self.fovy, self.clipping_near, self.clipping_far);

        //let target = Point3::<f32>::new(self.dir.x, self.dir.y, self.dir.z);
        let target = self.eye_pos + self.dir;

        self.view = Isometry3::look_at_rh(&self.eye_pos, &target, &self.up).to_homogeneous();

        self.projection_inverse = self.projection.inverse();
        self.view_inverse = self.view.try_inverse().unwrap();
    }

    pub fn is_default_cam(&self) -> bool
    {
        (
            approx_equal(self.eye_pos.x, DEFAULT_CAM_POS.x)
            &&
            approx_equal(self.eye_pos.y, DEFAULT_CAM_POS.y)
            &&
            approx_equal(self.eye_pos.z, DEFAULT_CAM_POS.z)
        )
        &&
        (
            approx_equal(self.dir.x, DEFAULT_CAM_DIR.x)
            &&
            approx_equal(self.dir.y, DEFAULT_CAM_DIR.y)
            &&
            approx_equal(self.dir.z, DEFAULT_CAM_DIR.z)
        )
        &&
        (
            approx_equal(self.up.x, DEFAULT_CAM_UP.x)
            &&
            approx_equal(self.up.y, DEFAULT_CAM_UP.y)
            &&
            approx_equal(self.up.z, DEFAULT_CAM_UP.z)
        )
        &&
        approx_equal(self.fovy, DEFAULT_FOVY.to_radians())
        &&
        approx_equal(self.clipping_near, DEFAULT_CLIPPING_NEAR)
        &&
        approx_equal(self.clipping_far, DEFAULT_CLIPPING_FAR)
    }

    pub fn set_cam_position(&mut self, eye_pos: Point3::<f32>, dir: Vector3::<f32>)
    {
        self.eye_pos = eye_pos;
        self.dir = dir;

        self.init_matrices();
    }

    pub fn webgpu_projection(&self) -> nalgebra::Matrix4<f32>
    {
        OPENGL_TO_WGPU_MATRIX * self.projection.to_homogeneous()
    }

    pub fn is_point_in_frustum(&self, point: &Point3<f32>) -> bool
    {
        let pv = self.projection.to_homogeneous() * self.view;
        let point_clip = pv * point.to_homogeneous();

        // Check if point is inside NDC space (Normalized Device Coordinates Space)
        point_clip.x.abs() <= point_clip.w && point_clip.y.abs() <= point_clip.w && point_clip.z.abs() <= point_clip.w
    }

    pub fn print(&self)
    {
        println!("name: {:?}", self.name);

        println!("id: {:?}", self.id);
        println!("name: {:?}", self.name);
        println!("enabled: {:?}", self.enabled);

        println!("viewport x: {:?}", self.viewport_x);
        println!("viewport y: {:?}", self.viewport_y);
        println!("viewport width: {:?}", self.viewport_width);
        println!("viewport height: {:?}", self.viewport_height);

        println!("resolution aspect_ratio: {:?}", self.resolution_aspect_ratio);

        println!("resolution width: {:?}", self.resolution_width);
        println!("resolution height: {:?}", self.resolution_height);

        println!("fov: {:?}", self.fovy);

        println!("eye_pos: {:?}", self.eye_pos);

        println!("up: {:?}", self.up);
        println!("dir: {:?}", self.dir);

        println!("clipping_near: {:?}", self.clipping_near);
        println!("clipping_far: {:?}", self.clipping_far);

        println!("projection: {:?}", self.projection);
        println!("view: {:?}", self.view);
    }

    pub fn print_short(&self)
    {
        println!(" - (CAMERA): id={} name={} enabled={} viewport=[x={}, y={}],[{}x{}], resolution={}x{}, fovy={} eye_pos={:?} near={}, far={}", self.id, self.name, self.enabled, self.viewport_x, self.viewport_y, self.viewport_width, self.viewport_height, self.resolution_width, self.resolution_height, self.fovy, self.eye_pos, self.clipping_near, self.clipping_far);
    }
}