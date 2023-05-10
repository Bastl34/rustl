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

    pub(crate) mod helper
    {
        pub(crate) mod buffer;
    }
}

mod state
{
    pub(crate) mod state;

    pub(crate) mod scene
    {
        pub(crate) mod manager
        {
            pub(crate) mod id_manager;
        }

        pub(crate) mod loader
        {
            pub(crate) mod wavefront;
        }

        pub(crate) mod components
        {
            pub(crate) mod component;
            pub(crate) mod transformation;
            pub(crate) mod mesh;
            pub(crate) mod material;
        }

        pub(crate) mod texture;
        pub(crate) mod camera;
        pub(crate) mod instance;
        pub(crate) mod node;
        pub(crate) mod scene;
    }

    pub(crate) mod gui
    {
        pub(crate) mod gui;
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