use anyhow::*;
use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use std::env;

fn main() -> Result<()>
{
    // This tells cargo to rerun this script if something in resources/ changes
    println!("cargo:rerun-if-changed=resources/*");

    let target = env::var("TARGET").unwrap();

    let out_dir;
    if target.contains("wasm32")
    {
        out_dir = "pkg".to_string();
    }
    else
    {
        out_dir = env::var("OUT_DIR")?;
    }

    let mut copy_options = CopyOptions::new();
    copy_options.overwrite = true;
    let mut paths_to_copy = Vec::new();
    paths_to_copy.push("resources/");
    copy_items(&paths_to_copy, out_dir, &copy_options)?;

    Ok(())
}