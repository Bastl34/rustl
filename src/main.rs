// release builds without the editor (scripts/build.mjs) open no console window on windows - --console brings it back
#![cfg_attr(all(windows, not(debug_assertions), not(feature = "editor")), windows_subsystem = "windows")]

use rustl::run;

fn main()
{
    #[cfg(all(windows, not(debug_assertions), not(feature = "editor")))]
    if std::env::args().any(|arg| arg == "--console")
    {
        show_console();
    }

    unsafe { std::env::set_var("RUST_BACKTRACE", "full") };
    run();
}

// the console of the terminal it was started from, else a new one
#[cfg(all(windows, not(debug_assertions), not(feature = "editor")))]
fn show_console()
{
    use windows_sys::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};

    unsafe
    {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0
        {
            AllocConsole();
        }
    }
}
