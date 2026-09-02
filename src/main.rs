use rustl::run;

fn main()
{
    unsafe { std::env::set_var("RUST_BACKTRACE", "full") };
    run();
}
