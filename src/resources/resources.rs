use std::fs;

use cfg_if::cfg_if;
use log::info;

use crate::helper::file::get_dirname;

pub const RESOURCES_DIR: &str = "resources";

#[cfg(target_arch = "wasm32")]
fn format_url(file_name: &str) -> reqwest::Url
{
    let window = web_sys::window().unwrap();
    let location = window.location();
    let mut origin = location.origin().unwrap();
    let pathname = location.pathname().unwrap();

    let current_dir: String = origin.to_string() + &pathname;
    let res_dir = get_dirname(current_dir.as_str()) + "/resources";

    let base = reqwest::Url::parse(&format!("{}/", res_dir,)).unwrap();
    base.join(file_name).unwrap()
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
            let path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR).join(file_name);
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
            let url = format_url(file_name);
            let txt = reqwest::blocking::get(url)?.text()?;
        }
        else
        {
            let path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR).join(file_name);
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
            let path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR).join(file_name);
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
            let url = format_url(file_name);
            let data = reqwest::blocking::get(url)?.bytes()?.to_vec();
        }
        else
        {
            let path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR).join(file_name);
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

    let full_path = std::path::Path::new(env!("OUT_DIR")).join(RESOURCES_DIR).join(path);

    let paths = fs::read_dir(full_path.clone()).unwrap();

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