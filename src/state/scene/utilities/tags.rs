#![allow(dead_code)]

use std::collections::{hash_map::Iter, HashMap};

use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TagData
{
    pub color: Vector3::<f32>,
    pub locked: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tags
{
    pub tags: HashMap<String, TagData>,
}

const DEFAULT_COLOR: Vector3::<f32> = Vector3::<f32>::new(0.12, 0.45, 0.88);
pub const DEFAULT_RED_COLOR: Vector3::<f32> = Vector3::<f32>::new(0.88, 0.12, 0.12);

impl Tags
{
    pub fn new() -> Tags
    {
        Tags
        {
            tags: HashMap::new()
        }
    }
    pub fn contains(&self, tag: &str) -> bool
    {
        let tag = tag.to_string();
        self.tags.contains_key(&tag)
    }

    pub fn insert(&mut self, tag: &str)
    {
        let tag = tag.to_string();
        self.tags.insert(tag, TagData { color: DEFAULT_COLOR, locked: false });
    }

    pub fn insert_with_color(&mut self, tag: &str, color: Vector3::<f32>)
    {
        let tag = tag.to_string();
        self.tags.insert(tag,  TagData { color, locked: false });
    }

    pub fn insert_with_color_locked(&mut self, tag: &str, color: Vector3::<f32>, locked: bool)
    {
        let tag = tag.to_string();
        self.tags.insert(tag, TagData { color, locked });
    }

    pub fn insert_locked(&mut self, tag: &str, locked: bool)
    {
        let tag = tag.to_string();
        self.tags.insert(tag, TagData { color: DEFAULT_COLOR, locked });
    }

    pub fn remove(&mut self, tag: &str)
    {
        let tag = tag.to_string();
        self.tags.remove(&tag);
    }

    pub fn iter(&self) -> Iter<'_, String, TagData>
    {
        self.tags.iter()
    }

}