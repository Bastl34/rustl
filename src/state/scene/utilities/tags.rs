#![allow(dead_code)]

use std::collections::HashMap;

use nalgebra::Vector3;
use serde::{Deserialize, Serialize, Serializer};

use super::origin::Origin;

#[derive(Clone, Serialize, Deserialize)]
pub struct TagData
{
    pub color: Vector3::<f32>,

    // runtime tags (set by code) can not be changed in the editor and are never saved
    #[serde(skip, default = "scene_origin")]
    pub origin: Origin,
}

// whatever is read from a file is a scene tag
fn scene_origin() -> Origin
{
    Origin::Scene
}

// serialized without the runtime tags
#[derive(Clone, Deserialize, Default)]
#[serde(transparent)]
pub struct Tags
{
    pub tags: HashMap<String, TagData>,
}

impl Serialize for Tags
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error>
    {
        serializer.collect_map(self.tags.iter().filter(|(_, data)| data.origin != Origin::Runtime))
    }
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
        self.tags.contains_key(tag)
    }

    pub fn contains_starts_with(&self, tag: &str) -> bool
    {
        self.tags.keys().any(|k| k.starts_with(tag))
    }

    // a runtime tag
    pub fn insert(&mut self, tag: &str)
    {
        self.insert_with_color_origin(tag, DEFAULT_COLOR, Origin::Runtime);
    }

    // a runtime tag
    pub fn insert_with_color(&mut self, tag: &str, color: Vector3::<f32>)
    {
        self.insert_with_color_origin(tag, color, Origin::Runtime);
    }

    pub fn insert_with_origin(&mut self, tag: &str, origin: Origin)
    {
        self.insert_with_color_origin(tag, DEFAULT_COLOR, origin);
    }

    pub fn insert_with_color_origin(&mut self, tag: &str, color: Vector3::<f32>, origin: Origin)
    {
        self.tags.insert(tag.to_string(), TagData { color, origin });
    }

    pub fn remove(&mut self, tag: &str)
    {
        self.tags.remove(tag);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &TagData)>
    {
        self.tags.iter()
    }

    // the tags of the scene file, sorted - what the editor saves
    pub fn scene_tags(&self) -> Vec<String>
    {
        let mut tags: Vec<String> = self.tags.iter().filter(|(_, data)| data.origin == Origin::Scene).map(|(tag, _)| tag.clone()).collect();
        tags.sort();
        tags
    }
}
