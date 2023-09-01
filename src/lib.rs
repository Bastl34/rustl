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

        pub(crate) mod texture;
        pub(crate) mod camera;
        pub(crate) mod light;
        pub(crate) mod instance;
        pub(crate) mod node;
        pub(crate) mod scene;
    }

    pub(crate) mod gui
    {
        pub(crate) mod gui;
        pub(crate) mod info_box;
        pub(crate) mod generic_items;
    }
}

mod window
{
    pub(crate) mod window;
}

mod interface
{
    pub(crate) mod main_interface;
}

mod helper
{
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