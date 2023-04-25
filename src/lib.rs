mod rendering
{
    pub(crate) mod wgpu;
    pub(crate) mod scene;
}

mod state
{
    pub(crate) mod state;
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

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub fn start()
{
    window::window::start();
}