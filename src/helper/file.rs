#![allow(dead_code)]

use std::{path::{PathBuf, Path}, env};

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