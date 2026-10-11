#![allow(dead_code)]

use std::{env, fs};

use cfg_if::cfg_if;

pub const RESOURCES_DIR: &str = "resources";
pub const RESOURCE_SCHEME: &str = "resources://";

#[cfg(target_arch = "wasm32")]
static RESOURCES_URL: std::sync::OnceLock<reqwest::Url> = std::sync::OnceLock::new();

// resolved from the page url - workers have no window, so the main thread must call this first (window::run does)
#[cfg(target_arch = "wasm32")]
pub fn resources_url() -> &'static reqwest::Url
{
    RESOURCES_URL.get_or_init(||
    {
        let href = web_sys::window().expect("resources url must be resolved on the main thread first").location().href().unwrap();
        reqwest::Url::parse(&href).unwrap().join(&format!("{}/", RESOURCES_DIR)).unwrap()
    })
}

#[cfg(target_arch = "wasm32")]
fn format_url(file_name: &str) -> reqwest::Url
{
    resources_url().join(file_name).unwrap()
}

// reqwest has no blocking client on the web - sync XHR works in workers and (deprecated, but allowed) on the main thread
#[cfg(target_arch = "wasm32")]
fn request_sync(method: &str, file_name: &str, binary: bool) -> anyhow::Result<web_sys::XmlHttpRequest>
{
    let js_err = |err: wasm_bindgen::JsValue| anyhow::anyhow!("{:?}", err);
    let in_worker = web_sys::window().is_none();

    let xhr = web_sys::XmlHttpRequest::new().map_err(js_err)?;
    xhr.open_with_async(method, format_url(file_name).as_str(), false).map_err(js_err)?;

    // the main thread can not set a response type for sync requests - x-user-defined keeps every byte in responseText
    if binary && in_worker
    {
        xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
    }
    else if binary
    {
        xhr.override_mime_type("text/plain; charset=x-user-defined").map_err(js_err)?;
    }

    xhr.send().map_err(js_err)?;

    let status = xhr.status().map_err(js_err)?;
    if !(200..300).contains(&status)
    {
        anyhow::bail!("{} {} failed with status {}", method, file_name, status);
    }

    Ok(xhr)
}

#[cfg(target_arch = "wasm32")]
fn response_bytes(xhr: &web_sys::XmlHttpRequest) -> anyhow::Result<Vec<u8>>
{
    let js_err = |err: wasm_bindgen::JsValue| anyhow::anyhow!("{:?}", err);

    if xhr.response_type() == web_sys::XmlHttpRequestResponseType::Arraybuffer
    {
        return Ok(js_sys::Uint8Array::new(&xhr.response().map_err(js_err)?).to_vec());
    }

    // x-user-defined maps byte 0x80..0xFF to U+F780..U+F7FF - the low byte is the original one
    let text = xhr.response_text().map_err(js_err)?.unwrap_or_default();
    Ok(text.chars().map(|c| c as u32 as u8).collect())
}

pub async fn load_string_async(file_name: &str) -> anyhow::Result<String>
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            let url = format_url(file_name);
            let txt = reqwest::get(url).await?.text().await?;
        }
        else
        {
            let path = get_path(&file_name);
            let txt = std::fs::read_to_string(path)?;
        }
    }

    Ok(txt)
}

pub fn load_string(file_name: &str) -> anyhow::Result<String>
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            let xhr = request_sync("GET", file_name, false)?;
            let txt = xhr.response_text().map_err(|err| anyhow::anyhow!("{:?}", err))?.unwrap_or_default();
        }
        else
        {
            let path = get_path(&file_name);
            let txt = std::fs::read_to_string(path)?;
        }
    }

    Ok(txt)
}

pub async fn load_binary_async(file_name: &str) -> anyhow::Result<Vec<u8>>
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            let url = format_url(file_name);
            let data = reqwest::get(url).await?.bytes().await?.to_vec();
        }
        else
        {
            let path = get_path(&file_name);
            let data = std::fs::read(path)?;
        }
    }

    Ok(data)
}

pub fn load_binary(file_name: &str) -> anyhow::Result<Vec<u8>>
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            let data = response_bytes(&request_sync("GET", file_name, true)?)?;
        }
        else
        {
            let path = get_path(&file_name);
            let data = std::fs::read(path)?;
        }
    }

    Ok(data)
}

pub fn read_files_recursive(path: &str) -> Vec<String>
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            return vec![];
        }
    }

    let full_path_str = get_path(&path);
    let full_path = std::path::Path::new(full_path_str.as_str());

    let paths = fs::read_dir(full_path);

    if paths.is_err()
    {
        return vec![];
    }

    let paths = paths.unwrap();

    let mut string_paths: Vec<String> = vec![];

    for entry in paths
    {
        if let Ok(entry) = entry
        {
            if let Ok(metadata) = entry.metadata()
            {
                if metadata.is_dir()
                {

                    let recursive_path = std::path::Path::new(path).join(entry.file_name());
                    let files = read_files_recursive(recursive_path.display().to_string().as_str());
                    string_paths.extend(files);
                }
                else
                {
                    string_paths.push(entry.path().display().to_string());
                }
            }
        }
    }

    // get relative path from resource directlry (if possible)
    let resource_path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR);
    let mut resource_path = resource_path.display().to_string();
    resource_path = resource_path.replace("\\", "/");

    string_paths = string_paths.iter().map(|item|
    {
        let item = item.replace("\\", "/");

        if item.len() > resource_path.len()
        {
            if &item[0..resource_path.len()] == resource_path
            {
                let new_item = &item[resource_path.len() + 1..];
                return new_item.to_string().clone();
            }
        }

        item.clone()
    }).collect();

    string_paths
}

pub fn exists(path: &str) -> bool
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            request_sync("HEAD", path, false).is_ok()
        }
        else
        {
            let full_path_str = get_path(&path);
            let path = std::path::Path::new(full_path_str.as_str());
            path.exists()
        }
    }
}


// A file inside the bundled resources as the path relative to them ("sounds/vehicle/skid.ogg"), like read_files_recursive lists them - None for other files.
pub fn to_resource_path(path: &str) -> Option<String>
{
    let path = path.replace('\\', "/");
    let bundled = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR);

    if !std::path::Path::new(&path).is_absolute()
    {
        if bundled.join(&path).exists()
        {
            return Some(path);
        }

        // relative to the working directory, like "resources/sounds/..."
        let stripped = path.strip_prefix(&format!("{}/", RESOURCES_DIR))?;
        return if bundled.join(stripped).exists() { Some(stripped.to_string()) } else { None };
    }

    // the bundled copy or the resources folder in the working directory - but only files the bundled copy has
    let target = std::fs::canonicalize(&path).ok()?;
    let roots = [Some(bundled.clone()), env::current_dir().ok().map(|dir| dir.join(RESOURCES_DIR))];

    for root in roots.into_iter().flatten()
    {
        if let Ok(root) = std::fs::canonicalize(root)
        {
            if let Ok(relative) = target.strip_prefix(&root)
            {
                let relative = relative.to_string_lossy().replace('\\', "/");
                if bundled.join(&relative).exists()
                {
                    return Some(relative);
                }
            }
        }
    }

    None
}

// packaged builds have the resources next to the executable (mac app bundle: Contents/Resources), the dev build uses the copy of build.rs
#[cfg(not(target_arch = "wasm32"))]
fn resource_roots() -> &'static [std::path::PathBuf]
{
    static ROOTS: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
    ROOTS.get_or_init(||
    {
        let mut roots = vec![];
        if let Some(exe_dir) = env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        {
            roots.push(exe_dir.join(RESOURCES_DIR));
            roots.push(exe_dir.join("..").join("Resources").join(RESOURCES_DIR));
        }
        roots.push(std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR));

        roots.retain(|root| root.is_dir());
        roots
    })
}

pub fn get_path(path: &str) -> String
{
    cfg_if!
    {
        if #[cfg(target_arch = "wasm32")]
        {
            path.to_string()
        }
        else
        {
            // absolute path
            if std::path::Path::new(path).is_absolute()
            {
                return path.to_string();
            }

            // resource path
            for root in resource_roots()
            {
                let resource_path = root.join(path);
                if resource_path.exists()
                {
                    return resource_path.to_string_lossy().to_string();
                }
            }

            // local path
            let local_path = env::current_dir().unwrap().join(path);
            local_path.to_string_lossy().to_string()
        }
    }
}
