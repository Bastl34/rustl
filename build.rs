use anyhow::*;
use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use std::env;

fn main() -> Result<()>
{
    // rerun if something in resources/ changes
    println!("cargo:rerun-if-changed=resources");

    // web build of a project with code: the project crate is the wasm module with its own start function (scripts/build.mjs)
    println!("cargo::rustc-check-cfg=cfg(rustl_external_start)");
    println!("cargo:rerun-if-env-changed=RUSTL_EXTERNAL_START");
    if env::var("RUSTL_EXTERNAL_START").is_ok()
    {
        println!("cargo:rustc-cfg=rustl_external_start");
    }

    // the web build gets its resources from scripts/build.mjs, which copies them into dist/web
    let target = env::var("TARGET").unwrap();
    if target.contains("wasm32")
    {
        return Ok(());
    }

    let out_dir = env::var("OUT_DIR")?;

    let mut copy_options = CopyOptions::new();
    copy_options.overwrite = true;
    let mut paths_to_copy = Vec::new();
    paths_to_copy.push("resources/");
    copy_items(&paths_to_copy, &out_dir, &copy_options)?;

    Ok(())
}
