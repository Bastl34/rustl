mod rendering
{
    pub(crate) mod wgpu;
    pub(crate) mod egui;
    pub(crate) mod pipeline;
    pub(crate) mod vertex_buffer;
    pub(crate) mod instance;
    pub(crate) mod texture;
    pub(crate) mod scene;
    pub(crate) mod camera;
    pub(crate) mod uniform;
    pub(crate) mod light;
    pub(crate) mod material;

    pub(crate) mod bind_groups
    {
        pub(crate) mod light_cam;
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

        pub(crate) mod components
        {
            pub(crate) mod component;
            pub(crate) mod transformation;
            pub(crate) mod mesh;
            pub(crate) mod material;
            pub(crate) mod alpha;
            pub(crate) mod transformation_animation;
        }

        pub(crate) mod camera_controller
        {
            pub(crate) mod camera_controller;
            pub(crate) mod fly_controller;
        }

        pub(crate) mod utilities
        {
            pub(crate) mod scene_utils;
        }

        pub(crate) mod texture;
        pub(crate) mod camera;
        pub(crate) mod light;
        pub(crate) mod instance;
        pub(crate) mod node;
        pub(crate) mod scene;
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
            pub(crate) mod main_frame;
            pub(crate) mod modals;
            pub(crate) mod dialogs;
            pub(crate) mod statistics;
            pub(crate) mod cameras;
            pub(crate) mod objects;
            pub(crate) mod materials;
            pub(crate) mod lights;
            pub(crate) mod scenes;
            pub(crate) mod rendering;
            pub(crate) mod textures;
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

}

mod window
{
    pub(crate) mod window;
}

mod interface
{
    pub(crate) mod main_interface;
    pub(crate) mod winit;
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

}

mod resources
{
    pub(crate) mod resources;
}

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub async fn start()
{
    window::window::start().await;
}