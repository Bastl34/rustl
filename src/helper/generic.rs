#![allow(dead_code)]

use web_time::SystemTime;
use web_time::UNIX_EPOCH;

use nalgebra::Point2;
use nalgebra::Point3;
use nalgebra::Vector3;

pub fn get_millis() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

pub fn get_secs() -> u64
{
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u64
}


pub fn match_by_include_exclude(str: &String, include: &Vec<String>, exclude: &Vec<String>) -> bool
{
    for inc in include
    {
        if str.find(inc).is_none()
        {
            return false;
        }
    }

    for ex in exclude
    {
        if str.find(ex).is_some()
        {
            return false;
        }
    }

    true
}

pub fn cut_str_to_length(s: &str, length: usize) -> String
{
    if s.len() <= length
    {
        return s.to_string();
    }

    let mut cut = s.chars().take(length).collect::<String>();
    cut.push_str("...");

    cut
}

pub fn cut_string_to_length(s: &String, length: usize) -> String
{
    let s = s.as_str();
    if s.len() <= length
    {
        return s.to_string();
    }

    let mut cut = s.chars().take(length).collect::<String>();
    cut.push_str("...");

    cut
}

pub fn vec3_as_array(p: &Vector3<f32>) -> [f32; 3]
{
    [p.x, p.y, p.z]
}

pub fn point2_as_array(p: &Point2<f32>) -> [f32; 2]
{
    [p.x, p.y]
}

pub fn point3_as_array(p: &Point3<f32>) -> [f32; 3]
{
    [p.x, p.y, p.z]
}

pub fn format_duration_secs(total_secs: u64) -> String
{
    let years   = total_secs / 31536000;
    let months  = (total_secs % 31536000) / 2592000;
    let days    = (total_secs % 31536000 % 2592000) / 86400;
    let hours   = (total_secs % 2592000 % 86400) / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs    = total_secs % 60;

    if years > 0
    {
        format!("{}y {}mo {}d {:02}h {:02}m {:02}s", years, months, days, hours, minutes, secs)
    }
    else if months > 0
    {
        format!("{}mo {}d {:02}h {:02}m {:02}s", months, days, hours, minutes, secs)
    }
    else if days > 0
    {
        format!("{}d {:02}h {:02}m {:02}s", days, hours, minutes, secs)
    }
    else if hours > 0
    {
        format!("{}h {:02}m {:02}s", hours, minutes, secs)
    }
    else
    {
        format!("{:02}m {:02}s", minutes, secs)
    }
}

pub fn cargo_dependencies() -> &'static [String]
{
    static DEPS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    DEPS.get_or_init(||
    {
        let cargo_toml = include_str!("../../Cargo.toml");
        let mut deps: Vec<String> = Vec::new();
        let mut in_deps = false;

        for raw in cargo_toml.lines()
        {
            let line = raw.trim();

            if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            {
                in_deps = section == "dependencies" || (section.starts_with("target.") && section.ends_with(".dependencies"));
                continue;
            }

            if !in_deps || line.is_empty() || line.starts_with('#') { continue; }

            if let Some(eq) = line.find('=')
            {
                let name = line[..eq].trim();
                if !name.is_empty() && !deps.iter().any(|d| d == name)
                {
                    deps.push(name.to_string());
                }
            }
        }

        deps.sort();
        deps
    })
}