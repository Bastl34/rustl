#![allow(dead_code)]

use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bytemuck::cast_slice;
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