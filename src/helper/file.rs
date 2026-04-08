#![allow(dead_code)]

use std::{path::{PathBuf, Path}, env};
use std::fs::File;
use std::io::prelude::*;

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

pub fn normalize_path_separators(path: &str) -> String
{
    path.replace('\\', "/")
}

pub fn resolve_relative_path(base_file: &str, relative: &str) -> String
{
    let base_dir = match std::path::Path::new(base_file).parent()
    {
        Some(p) => p,
        None => return relative.to_string(),
    };

    normalize_path_separators(&base_dir.join(relative).to_string_lossy())
}

pub fn make_relative_path(base_file: &str, target: &str) -> Option<String>
{
    let base_dir = std::fs::canonicalize(std::path::Path::new(base_file).parent()?).ok()?;
    let abs_target = std::fs::canonicalize(target).ok()?;

    let mut base_parts = base_dir.components().peekable();
    let mut target_parts = abs_target.components().peekable();

    // skip common prefix
    while base_parts.peek() == target_parts.peek() && base_parts.peek().is_some()
    {
        base_parts.next();
        target_parts.next();
    }

    let mut rel = std::path::PathBuf::new();
    for _ in base_parts
    {
        rel.push("..");
    }

    for part in target_parts
    {
        rel.push(part);
    }

    Some(normalize_path_separators(&rel.to_string_lossy()))
}