// the editor with the engine as shared library - it can load the code of projects (cargo dev)
fn main()
{
    unsafe { std::env::set_var("RUST_BACKTRACE", "full") };
    rustl_dylib::run();
}
