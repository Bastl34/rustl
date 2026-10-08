#[cfg(not(target_arch = "wasm32"))]
use std::thread as thread;
#[cfg(target_arch = "wasm32")]
use wasm_thread as thread;

use web_time::Duration;

pub use thread::JoinHandle as ThreadResult;

pub fn spawn_thread<F: FnOnce() + Send + 'static>(func: F) -> ThreadResult<()>
{
    thread::spawn(func)
}

pub fn sleep_millis(millis: u64)
{
    thread::sleep(Duration::from_millis(millis));
}