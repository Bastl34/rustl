#[cfg(not(target_arch = "wasm32"))]
use std::thread as thread;
#[cfg(target_arch = "wasm32")]
use wasm_thread as thread;

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use web_time::Duration;

pub use thread::JoinHandle as ThreadResult;

// incremented on every switch between edit and run - odd while running
static RUN_SESSION: AtomicU64 = AtomicU64::new(0);

thread_local!
{
    static THREAD_RUN_SESSION: Cell<Option<u64>> = const { Cell::new(None) };
}

// run session of the thread start - the main thread uses the current one
pub fn run_session() -> u64
{
    THREAD_RUN_SESSION.get().unwrap_or_else(|| RUN_SESSION.load(Ordering::SeqCst))
}

pub fn set_running(running: bool)
{
    if (RUN_SESSION.load(Ordering::SeqCst) % 2 == 1) != running
    {
        RUN_SESSION.fetch_add(1, Ordering::SeqCst);
    }
}

// true for work of an ended run (e.g. a load that finishes after stop)
pub fn is_ended_run_session(session: u64) -> bool
{
    session % 2 == 1 && session != RUN_SESSION.load(Ordering::SeqCst)
}

pub fn spawn_thread<F: FnOnce() + Send + 'static>(func: F) -> ThreadResult<()>
{
    let session = run_session();
    thread::spawn(move ||
    {
        THREAD_RUN_SESSION.set(Some(session));
        func()
    })
}

pub fn sleep_millis(millis: u64)
{
    thread::sleep(Duration::from_millis(millis));
}