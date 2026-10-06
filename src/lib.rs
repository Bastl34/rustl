pub mod rendering
{
    pub mod wgpu;
    pub mod egui;
    pub mod pipeline;
    pub mod compute_pipeline;
    pub mod vertex_buffer;
    pub mod instance;
    pub mod texture;
    pub mod state;
    pub mod scene;
    pub mod camera;
    pub mod light;
    pub mod shadow;
    pub mod material;
    pub mod skeleton;
    pub mod morph_target;
    pub mod bounding_boxes;
    pub mod debug_volumes;
    pub mod visibility;
    pub mod hzb_cull_buffer;
    pub mod draw_slots;
    pub mod gpu_timer;

    pub mod bind_groups
    {
        pub mod uniform;
        pub mod storage;
        pub mod light_cam_scene;
        pub mod skeleton_morph_target;
        pub mod single_binding_group;
        pub mod debug_volumes;
        pub mod depth_export;
        pub mod hzb_downsample;
        pub mod hzb_occlusion_check;
        pub mod ssao;
    }

    pub mod helper
    {
        pub mod buffer;
    }
}

pub mod state
{
    pub mod state;

    pub mod helper
    {
        pub mod render_item;
    }

    pub mod scene
    {
        pub mod manager
        {
            pub mod id_manager;
        }

        pub mod loader
        {
            pub mod wavefront;
            pub mod gltf;
            pub mod asset_container;
            pub mod loader;
        }

        pub mod exporter
        {
            pub mod json;
            pub mod serialization_helper;
        }

        pub mod components
        {
            pub mod component;
            pub mod transformation;
            pub mod mesh;
            pub mod material;
            pub mod alpha;
            pub mod transformation_animation;
            pub mod joint;
            pub mod animation;
            pub mod morph_target;
            pub mod morph_target_animation;
            pub mod animation_blending;
            pub mod look_at;
            pub mod sound;
            pub mod delay;
        }

        pub mod physics
        {
            pub mod contacts;
            pub mod physics_world;
        }

        pub mod scene_controller
        {
            pub mod scene_controller;
            pub mod char_controller;
            pub mod vehicle_controller;

            pub mod vehicle
            {
                pub mod engine;
                pub mod engine_sound;
                pub mod presets;
                pub mod tire_marks;
            }
        }

        pub mod camera_controller
        {
            pub mod camera_controller;
            pub mod fly_controller;
            pub mod pan_controller;
            pub mod target_rotation_controller;
            pub mod follow_controller;
            pub mod path_controller;
        }

        pub mod utilities
        {
            pub mod scene_utils;
            pub mod extras;
            pub mod tags;
        }

        pub mod camera;
        pub mod light;
        pub mod instance;
        pub mod layers;
        pub mod node;
        pub mod scene;
    }

    pub mod resources
    {
        pub mod utilities
        {
            pub mod resource_utils;
        }

        pub mod texture;
        pub mod sound_source;
        pub mod mesh_resource;
    }

    pub mod project
    {
        pub mod project;
        pub mod loader;
    }


}

pub mod gui
{
    // generic egui widgets - also used by scene components for their inspector UI,
    // so this stays available without the editor
    pub mod helper
    {
        pub mod info_box;
        pub mod property_items;
        #[cfg(feature = "editor")]
        pub mod generic_items;
    }

    #[cfg(feature = "editor")]
    pub mod editor
    {
        pub mod editor;
        pub mod editor_state;
        pub mod editor_project;
        pub mod project_code;
        pub mod recent_projects;
        pub mod settings;
        pub mod helper;
        pub mod gizmo;
        pub mod grid;
        pub mod box_select;
        pub mod preview_scene;

        pub mod ui
        {
            pub mod helper
            {
                pub mod ui_helper;
            }

            pub mod main_frame;
            pub mod modals;
            pub mod dialogs;
            pub mod export;
            pub mod statistics;
            pub mod cameras;
            pub mod objects;
            pub mod materials;
            pub mod lights;
            pub mod scenes;
            pub mod scene_tabs;
            pub mod run_mode_bar;
            pub mod general;
            pub mod project;
            pub mod debug;
            pub mod textures;
            pub mod sound;
            pub mod mesh;
            pub mod assets;
            pub mod console;
            pub mod code_editor;
            pub mod help;
        }
    }
}

pub mod input
{
    pub mod input_manager;

    pub mod press_state;
    pub mod input_point;

    pub mod keyboard;
    pub mod mouse;
    pub mod touch;
    pub mod gamepad;

    pub mod input_binding;
}

pub mod output
{
    pub mod audio_device;
}

pub mod window
{
    pub mod window;
}

pub mod interface
{
    pub mod main_interface;
    pub mod winit;
    pub mod gilrs;


    pub mod context;
    pub mod app;
    pub mod app_dummy;
}

pub mod helper
{
    pub mod concurrency
    {
        pub mod thread;
        pub mod execution_queue;
    }

    pub mod generic;
    pub mod file;
    pub mod math;
    pub mod image;
    pub mod crypto;
    pub mod consumable;
    pub mod change_tracker;
    pub mod platform;
    pub mod easing;
    pub mod curve;
    pub mod stopwatch;
    pub mod asset_path_descriptor;
    pub mod option_or_id;
    pub mod console_log;
    pub mod observable;
}

pub mod resources
{
    pub mod resources;
}

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

// web build of a project with code: the project crate has the start function and calls run_app (build.rs: rustl_external_start)
#[cfg_attr(all(target_arch="wasm32", not(rustl_external_start)), wasm_bindgen(start))]
pub fn run()
{
    window::window::run();
}

// crates of the engine api - project code uses them from here, so the versions match
#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen;
pub use nalgebra;
pub use egui;
pub use winit;
pub use wgpu;
pub use rapier3d;
pub use parry3d;
pub use serde;
pub use serde_json;
pub use typetag;
pub use log;

// use rustl::prelude::*;
pub mod prelude
{
    pub use crate::interface::app::App;
    pub use crate::interface::context::Context;
    pub use crate::state::state::State;
    pub use crate::state::scene::scene::Scene;
    pub use crate::state::scene::node::{Node, NodeItem};
    pub use crate::state::scene::components::transformation::Transformation;
    pub use crate::{console_log, console_warning, console_error, console_success, console_debug, component_downcast, component_downcast_mut};
    pub use nalgebra::{Point2, Point3, Vector2, Vector3, Vector4};
}

// the wiring of a project app (src/app.rs): rustl::app!(Game::default());
// the engine shared library in the editor, the factory the editor loads and the start of the web export
#[macro_export]
macro_rules! app
{
    ($create:expr) =>
    {
        #[cfg(feature = "dynamic")]
        #[allow(unused_imports)]
        use rustl_dylib as _;

        #[unsafe(no_mangle)]
        pub fn rustl_create_app() -> Box<dyn $crate::interface::app::App>
        {
            Box::new($create)
        }

        // wasm-bindgen of the engine - the project needs no dependency on it
        #[cfg(target_arch = "wasm32")]
        #[$crate::wasm_bindgen::prelude::wasm_bindgen(start, wasm_bindgen = $crate::wasm_bindgen)]
        pub fn rustl_start()
        {
            $crate::run_app(rustl_create_app);
        }
    };
}

// app entry point (for apps with code)
pub fn run_app(factory: interface::app::AppFactory)
{
    interface::app::set_project_app_factory(factory);
    window::window::run();
}