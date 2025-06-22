#![allow(dead_code)]

use std::{path::{PathBuf, Path}, env};
use std::fs::File;
use std::io::prelude::*;

use egui::epaint::tessellator::path;

pub fn get_current_working_dir() -> std::io::Result<PathBuf>
{
    env::current_dir()
}

pub fn get_current_working_dir_str() -> String
{
    let cwd = get_current_working_dir().unwrap();
    String::from(cwd.to_string_lossy())
}

pub fn get_dirname(path: &str) -> String
{
    let path = Path::new(path);
    let parent = path.parent();

    match parent
    {
        Some(p) => { return p.display().to_string() },
        None =>  { return "".to_string(); },
    }
}

pub fn get_stem(path: &str) -> String
{
    if let Some(stem) = Path::new(&path).file_stem()
    {
        return String::from(stem.to_string_lossy());
    }

    "".to_string()
}

pub fn get_extension(path: &str) -> String
{
    if let Some(extension) = Path::new(&path).extension()
    {
        return String::from(extension.to_string_lossy());
    }

    "".to_string()
}

pub fn is_absolute(path: &str) -> bool
{
    Path::new(path).is_absolute()
}

pub fn write_string_to_tile(path: &str, content: String) -> std::io::Result<()>
{
    let mut file = File::create(path)?;
    file.write(content.as_bytes())?;
    Ok(())
}