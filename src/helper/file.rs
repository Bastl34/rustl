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

/*
pub fn read_files(path: &str) -> Vec<String>
{
    let paths = fs::read_dir(path).unwrap();

    let mut string_paths = vec![];

    for path in paths
    {
        string_paths.push(path.unwrap().path().display().to_string());
    }

    string_paths
}

pub fn read_files_recursive(path: &str) -> Vec<String>
{
    let paths = fs::read_dir(path).unwrap();

    let mut string_paths = vec![];

    for entry in paths
    {
        if let Ok(entry) = entry
        {
            // Here, `entry` is a `DirEntry`.
            if let Ok(metadata) = entry.metadata()
            {
                // Now let's show our entry's permissions!
                //println!("{:?}: {:?}", entry.path(), metadata.permissions());
                println!("{:?}: {:?}", entry.path(), metadata.is_dir());
            }
        }
    }

    string_paths
}
 */