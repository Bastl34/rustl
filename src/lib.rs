mod rendering
{
    pub(crate) mod wgpu;
    pub(crate) mod pipeline;
    pub(crate) mod buffer;
    pub(crate) mod texture;
    pub(crate) mod scene;
    pub(crate) mod camera;
}

mod state
{
    pub(crate) mod state;

    pub(crate) mod scene
    {
        pub(crate) mod camera;
    }
}

mod window
{
    pub(crate) mod window;
    pub(crate) mod egui;
}

mod interface
{
    pub(crate) mod main_interface;
}

mod helper
{
    pub(crate) mod file;
    pub(crate) mod math;
}

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub fn start()
{
    window::window::start();
}