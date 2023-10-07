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

/*
pub fn load_string(file_name: &str) -> Option<String>
{
    let res = pollster::block_on(load_string_async(file_name));

    if res.is_ok()
    {
        return Some(res.unwrap())
    }

    None
}

pub fn load_binary(file_name: &str) -> Option<Vec<u8>>
{
    let res = pollster::block_on(load_binary_async(file_name));

    if res.is_ok()
    {
        return Some(res.unwrap())
    }

    None
}
*/