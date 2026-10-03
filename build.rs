use anyhow::*;
use fs_extra::copy_items;
use fs_extra::dir::CopyOptions;
use std::{env, fs::File, path::Path};

// the rustl logo for now - later the app icon of the project (scripts/build.mjs passes RUSTL_APP_ICON)
const DEFAULT_APP_ICON: &str = "resources/designs/logo/logo.png";

fn main() -> Result<()>
{
    // This tells cargo to rerun this script if something in resources/ changes
    println!("cargo:rerun-if-changed=resources/*");

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

    if env::var("CARGO_CFG_TARGET_OS")? == "windows"
    {
        windows_resources(&out_dir);
    }

    Ok(())
}

// icon and name of the exe - a missing resource compiler (rc.exe of the windows sdk) only costs the icon, not the build
fn windows_resources(out_dir: &str)
{
    println!("cargo:rerun-if-env-changed=RUSTL_APP_NAME");
    println!("cargo:rerun-if-env-changed=RUSTL_APP_ICON");

    let name = env::var("RUSTL_APP_NAME").unwrap_or("rustl".to_string());
    let icon = env::var("RUSTL_APP_ICON").unwrap_or(DEFAULT_APP_ICON.to_string());
    println!("cargo:rerun-if-changed={}", icon);

    let ico = Path::new(out_dir).join("app.ico");
    if let Err(err) = write_ico(&icon, &ico)
    {
        println!("cargo:warning=app icon {}: {}", icon, err);
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(&ico.to_string_lossy());
    resource.set("FileDescription", &name);
    resource.set("ProductName", &name);

    if let Err(err) = resource.compile()
    {
        println!("cargo:warning=exe icon not embedded (resource compiler): {}", err);
    }
}

fn write_ico(png: &str, ico: &Path) -> Result<()>
{
    let image = image::open(png)?;

    let mut frames = vec![];
    for size in [256, 128, 64, 48, 32, 16]
    {
        let resized = image.resize_exact(size, size, image::imageops::FilterType::Lanczos3).to_rgba8();
        frames.push(image::codecs::ico::IcoFrame::as_png(resized.as_raw(), size, size, image::ExtendedColorType::Rgba8)?);
    }

    image::codecs::ico::IcoEncoder::new(File::create(ico)?).encode_images(&frames)?;
    Ok(())
}
