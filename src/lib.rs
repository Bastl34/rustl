mod rendering
{
    pub(crate) mod wgpu;
    pub(crate) mod egui;
    pub(crate) mod pipeline;
    pub(crate) mod compute_pipeline;
    pub(crate) mod vertex_buffer;
    pub(crate) mod instance;
    pub(crate) mod texture;
    pub(crate) mod state;
    pub(crate) mod scene;
    pub(crate) mod camera;
    pub(crate) mod light;
    pub(crate) mod material;
    pub(crate) mod skeleton;
    pub(crate) mod morph_target;
    pub(crate) mod bounding_boxes;
    pub(crate) mod visibility;

    pub(crate) mod bind_groups
    {
        pub(crate) mod uniform;
        pub(crate) mod storage;
        pub(crate) mod light_cam_scene;
        pub(crate) mod skeleton_morph_target;
        pub(crate) mod single_binding_group;
        pub(crate) mod depth_export;
        pub(crate) mod hzb_downsample;
        pub(crate) mod occlusion;
    }

    pub(crate) mod helper
    {
        pub(crate) mod buffer;
    }
}

mod state
{
    pub(crate) mod state;

    pub(crate) mod helper
    {
        pub(crate) mod render_item;
    }

    pub(crate) mod scene
    {
        pub(crate) mod manager
        {
            pub(crate) mod id_manager;
        }

        pub(crate) mod loader
        {
            pub(crate) mod wavefront;
            pub(crate) mod gltf;
        }

        pub(crate) mod exporter
        {
            pub(crate) mod json;
            pub(crate) mod serialization_helper;
        }

        pub(crate) mod components
        {
            pub(crate) mod component;
            pub(crate) mod transformation;
            pub(crate) mod mesh;
            pub(crate) mod material;
            pub(crate) mod alpha;
            pub(crate) mod transformation_animation;
            pub(crate) mod joint;
            pub(crate) mod animation;
            pub(crate) mod morph_target;
            pub(crate) mod morph_target_animation;
            pub(crate) mod animation_blending;
            pub(crate) mod look_at;
            pub(crate) mod sound;
            pub(crate) mod delay;
        }

        pub(crate) mod scene_controller
        {
            pub(crate) mod scene_controller;
            pub(crate) mod generic_controller;
            pub(crate) mod char_controller;
        }

        pub(crate) mod camera_controller
        {
            pub(crate) mod camera_controller;
            pub(crate) mod fly_controller;
            pub(crate) mod target_rotation_controller;
            pub(crate) mod follow_controller;
        }

        pub(crate) mod utilities
        {
            pub(crate) mod scene_utils;
            pub(crate) mod extras;
            pub(crate) mod tags;
        }

        pub(crate) mod camera;
        pub(crate) mod light;
        pub(crate) mod instance;
        pub(crate) mod node;
        pub(crate) mod scene;
    }

    pub(crate) mod resources
    {
        pub(crate) mod utilities
        {
            pub(crate) mod resource_utils;
        }

        pub(crate) mod texture;
        pub(crate) mod sound_source;
        pub(crate) mod mesh_resource;
    }

    pub(crate) mod gui
    {
        pub(crate) mod helper
        {
            pub(crate) mod info_box;
            pub(crate) mod generic_items;
        }

        pub(crate) mod editor
        {
            pub(crate) mod editor;
            pub(crate) mod editor_state;
            pub(crate) mod helper;
            pub(crate) mod gizmo;
            pub(crate) mod grid;

            pub(crate) mod ui
            {
                pub(crate) mod main_frame;
                pub(crate) mod modals;
                pub(crate) mod dialogs;
                pub(crate) mod statistics;
                pub(crate) mod cameras;
                pub(crate) mod objects;
                pub(crate) mod materials;
                pub(crate) mod lights;
                pub(crate) mod scenes;
                pub(crate) mod general;
                pub(crate) mod textures;
                pub(crate) mod sound;
                pub(crate) mod mesh;
                pub(crate) mod assets;
                pub(crate) mod console;
            }
        }
    }
}

pub(crate) mod input
{
    pub(crate) mod input_manager;

    pub(crate) mod press_state;
    pub(crate) mod input_point;

    pub(crate) mod keyboard;
    pub(crate) mod mouse;
    pub(crate) mod touch;
    pub(crate) mod gamepad;
}

pub(crate) mod output
{
    pub(crate) mod audio_device;
}

mod window
{
    pub(crate) mod window;
}

mod interface
{
    pub(crate) mod main_interface;
    pub(crate) mod winit;
    pub(crate) mod gilrs;


    pub(crate) mod context;
    pub(crate) mod app;
    pub(crate) mod app_dummy;
}

mod helper
{
    pub(crate) mod concurrency
    {
        pub(crate) mod thread;
        pub(crate) mod execution_queue;
    }

    pub(crate) mod generic;
    pub(crate) mod file;
    pub(crate) mod math;
    pub(crate) mod image;
    pub(crate) mod crypto;
    pub(crate) mod consumable;
    pub(crate) mod change_tracker;
    pub(crate) mod platform;
    pub(crate) mod easing;
    pub(crate) mod stopwatch;
    pub(crate) mod asset_path_descriptor;
    pub(crate) mod option_or_id;
    pub(crate) mod console_log;
}

mod resources
{
    pub(crate) mod resources;
}

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub fn run()
{
    window::window::run();
}