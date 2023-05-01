mod rendering
{
    pub(crate) mod wgpu;
    pub(crate) mod egui;
    pub(crate) mod pipeline;
    pub(crate) mod buffer;
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
        pub(crate) mod camera;
        pub(crate) mod instance;
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
}

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub fn start()
{
    window::window::start();
}