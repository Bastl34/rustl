#![allow(dead_code)]

use std::{path::{PathBuf, Path}, env};
use std::fs::File;
use std::io::prelude::*;

use crate::console_error;

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

pub fn sanitize_filename(name: &str) -> String
{
    name.chars()
        .map(|c| match c
        {
            ' ' => '_',
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c => c,
        })
        .collect()
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

// target relative to the folder of base_file - the absolute target on another drive
pub fn make_relative_path(base_file: &str, target: &str) -> Option<String>
{
    let base_dir = std::fs::canonicalize(std::path::Path::new(base_file).parent()?).ok()?;
    let abs_target = std::fs::canonicalize(target).ok()?;

    Some(relative_path(&base_dir, &abs_target).unwrap_or_else(|| normalize_path_separators(&abs_target.to_string_lossy())))
}

// from_dir -> to as "../x/y" ("" for the same folder) - None on another drive, there is no relative path
pub fn relative_path(from_dir: &Path, to: &Path) -> Option<String>
{
    let from_parts: Vec<_> = from_dir.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    let common = from_parts.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();

    if common == 0
    {
        return None;
    }

    let mut parts = vec!["..".to_string(); from_parts.len() - common];
    parts.extend(to_parts[common..].iter().map(|part| part.as_os_str().to_string_lossy().to_string()));

    Some(parts.join("/"))
}

// the path as the file system spells it (drive letter case, no \\?\ prefix) - absolute if it does not exist (yet)
pub fn real_path(path: &Path) -> PathBuf
{
    match std::fs::canonicalize(path)
    {
        Ok(real) =>
        {
            let real = real.to_string_lossy().to_string();
            PathBuf::from(real.strip_prefix(r"\\?\").unwrap_or(&real))
        },
        Err(_) => std::path::absolute(path).unwrap_or(path.to_path_buf()),
    }
}

// "src/app.rs" below dir - one join per part, windows tools do not take the / of the relative path
pub fn join_relative(dir: &Path, relative: &str) -> PathBuf
{
    relative.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
}

// a file or folder like a double click in the file manager
pub fn open_with_default_app(path: &Path)
{
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";

    if let Err(err) = std::process::Command::new(program).arg(path).spawn()
    {
        console_error!("can not open {}: {}", path.display(), err);
    }
}