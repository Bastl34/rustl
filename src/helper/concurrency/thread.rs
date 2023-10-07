#[cfg(not(target_arch = "wasm32"))]
use std::thread;
#[cfg(target_arch = "wasm32")]
use wasm_thread as thread;

//pub fn spawn_thread(func: fn())
//pub fn spawn_thread<F: Fn() + Send + 'static>(func: F)
pub fn spawn_thread<F: Fn() + Send + Sync + 'static>(func: F)
{
    thread::spawn(func);
}