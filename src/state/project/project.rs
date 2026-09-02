#![allow(dead_code)]
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::state::scene::exporter::serialization_helper::default_true;
use crate::state::scene::exporter::serialization_helper::is_true;
use crate::state::scene::exporter::serialization_helper::is_false;
use crate::state::state::State;

/// Node extra flag: reuse already loaded materials with the same name instead of duplicating them.
pub const RESUSE_MATERIALS_TAG: &str = "reuse_materials_by_name";

const PROJECT_FILE_VERSION: &str = "1.0.0";

pub type ProjectDoneCallback = Option<Box<dyn FnOnce(&mut State) + Send + Sync + 'static>>;

// ******************** structs ********************

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectFormat
{
    pub generator: String,
    pub version: String,
}

impl Default for EditorProjectFormat
{
    fn default() -> Self
    {
        EditorProjectFormat
        {
            generator: format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).to_string(),
            version: PROJECT_FILE_VERSION.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectData
{
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub license: String,
    pub url: String,

    pub build: u32,

    #[serde(default)]
    pub editing_time_secs: u64,
}

impl Default for EditorProjectData
{
    fn default() -> Self
    {
        EditorProjectData
        {
            name: "Untitled".to_string(),
            version: "0.0.1".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            license: "".to_string(),
            url: "".to_string(),

            build: 1,
            editing_time_secs: 0,
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProjectSceneRef
{
    pub path: String,

    #[serde(default, skip_serializing_if = "is_false")]
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorProject
{
    pub format: EditorProjectFormat,
    pub project: EditorProjectData,

    pub scenes: Vec<EditorProjectSceneRef>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorScene
{
    pub name: String,

    #[serde(default, skip_serializing)]
    pub active: bool,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<EditorObject>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorObjectOptions
{
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub visible: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_materials_by_name: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EditorObject
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    pub name: String,
    pub options: EditorObjectOptions,

    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub rotation_quat: Option<[f32; 4]>,
    pub scale: [f32; 3],

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<EditorObject>,
}


// ******************** loading state ********************

/// Resets the shared "is loading" flag when the loading task ends (also on early return/panic).
pub struct LoadingGuard(pub Arc<RwLock<bool>>);

impl Drop for LoadingGuard
{
    fn drop(&mut self)
    {
        *self.0.write().unwrap() = false;
    }
}