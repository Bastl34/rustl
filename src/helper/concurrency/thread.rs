use instant::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::thread as thread;
#[cfg(target_arch = "wasm32")]
use wasm_thread as thread;

pub use thread::JoinHandle as ThreadResult;

/*
pub struct ThreadResult
{
    handle: JoinHandle<()>
}

impl ThreadResult
{
    pub fn join(&mut self) -> std::thread::Result<()>
    {
        self.handle.join()
    }
}
*/

//pub fn spawn_thread<F: Fn() + Send + Sync + 'static, T: Send + 'static>(func: F) -> ThreadResult<T> where F: FnOnce() -> T
//pub fn spawn_thread<F: Fn() + Send + Sync + 'static, T: Send + 'static>(func: F) -> ThreadResult<T>
pub fn spawn_thread<F: Fn() + Send + Sync + 'static>(func: F) -> ThreadResult<()>
{
    thread::spawn(func)
    /*
    ThreadResult
    {
        handle: thread::spawn(func)
    }
    */
}

pub fn sleep_millis(millis: u64)
{
    thread::sleep(Duration::from_millis(millis));
}