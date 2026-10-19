#![allow(dead_code)]
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::state::scene::exporter::serialization_helper::is_false;
use crate::state::scene::node::NodeSettings;
use crate::state::scene::physics::physics_world::PhysicsWorldSettings;
use crate::state::state::{InputSettings, State, WindowSettings};

/// Scene extra: the saved editor cameras (json) - applied by the editor when it creates its cameras.
pub const EDITOR_VIEW_EXTRA: &str = "editor_view";

/// Node extra flag: reuse already loaded materials with the same name instead of duplicating them.
pub const RESUSE_MATERIALS_TAG: &str = "reuse_materials_by_name";

const PROJECT_FILE_VERSION: &str = "1.0.0";

pub type ProjectDoneCallback = Option<Box<dyn FnOnce(&mut State) + Send + Sync + 'static>>;

// ******************** structs ********************

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectFileFormat
{
    pub generator: String,
    pub version: String,
}

impl Default for ProjectFileFormat
{
    fn default() -> Self
    {
        ProjectFileFormat
        {
            generator: format!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")).to_string(),
            version: PROJECT_FILE_VERSION.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectData
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

    #[serde(default, skip_serializing_if = "ProjectExportDirs::is_empty")]
    pub export_dirs: ProjectExportDirs,
}

// target folders of the editor export per platform - empty: dist/<platform>
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ProjectExportDirs
{
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub web: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub windows: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub linux: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mac: String,
}

impl ProjectExportDirs
{
    pub fn is_empty(&self) -> bool
    {
        self.web.is_empty() && self.windows.is_empty() && self.linux.is_empty() && self.mac.is_empty()
    }
}

impl Default for ProjectData
{
    fn default() -> Self
    {
        ProjectData
        {
            name: "Untitled".to_string(),
            version: "0.0.1".to_string(),
            author: "".to_string(),
            description: "".to_string(),
            license: "".to_string(),
            url: "".to_string(),

            build: 1,
            editing_time_secs: 0,

            export_dirs: ProjectExportDirs::default(),
        }
    }
}


#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectSceneRef
{
    pub path: String,

    #[serde(default, skip_serializing_if = "is_false")]
    pub active: bool,

    // loaded with the project even when not active
    #[serde(default, skip_serializing_if = "is_false")]
    pub preload: bool,
}

// the global settings of the engine - missing ones fall back to the defaults on load
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ProjectSettings
{
    // serde of the runtime Rendering as it is (without the debug views)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendering: Option<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioSettings>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowSettings>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSettings>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AudioSettings
{
    pub volume: f32,
}

impl Default for AudioSettings
{
    fn default() -> Self
    {
        AudioSettings { volume: 1.0 }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectFile
{
    pub format: ProjectFileFormat,
    pub project: ProjectData,

    #[serde(default)]
    pub settings: ProjectSettings,

    pub scenes: Vec<ProjectSceneRef>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SceneFile
{
    pub name: String,

    #[serde(default, skip_serializing)]
    pub active: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SceneSettings>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physics: Option<PhysicsWorldSettings>,

    // scene tags set in the editor - the runtime ones of the code are not saved
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    // serde of the editor cameras - ignored without the editor
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub editor_cameras: Vec<serde_json::Value>,

    // the sound resources - loaded before the controllers, which refer to them by uuid
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sounds: Vec<SceneSound>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<SceneObject>,

    // serde of the runtime types as they are - parsed one by one, so a broken entry only drops itself
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cameras: Vec<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lights: Vec<serde_json::Value>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controller: Vec<serde_json::Value>,
}

// the scene data without the environment texture
#[derive(Serialize, Deserialize, Clone)]
pub struct SceneSettings
{
    pub max_lights: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gamma: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ibl_diffuse_intensity: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SceneSound
{
    pub uuid: String,
    pub name: String,
    pub source: String, // like an object source: relative to the project file or "resources://..."
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SceneObjectOptions
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reuse_materials_by_name: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,

    // the node settings as a whole (visible, locked, collision, culling, physics, ...)
    #[serde(default)]
    pub settings: NodeSettings,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SceneObject
{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    // kept so cameras and controllers can find their node again
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,

    pub name: String,
    pub options: SceneObjectOptions,

    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub rotation_quat: Option<[f32; 4]>,
    pub scale: [f32; 3],

    // components added in the editor (sounds, ...) - serde of the runtime types, parsed one by one
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<serde_json::Value>,

    // node extras (game data of converters, ...): bool, number, string or an array of 2 to 4 numbers
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extras: serde_json::Map<String, serde_json::Value>,

    // node tags set in the editor or by converters - the runtime ones of the code are not saved
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<SceneObject>,
}


// ******************** loading state ********************

// what the scene loader works on - read it for a loading screen: state.scene_loading.read().unwrap()
#[derive(Clone, Default, Debug)]
pub struct SceneLoadingStatus
{
    // scenes of the running loads - empty when nothing loads
    pub scenes: Vec<(u32, String)>,

    // the file (or object name) loaded right now
    pub current: String,

    // the finished files of the running loads, oldest first
    pub loaded: Vec<String>,

    // objects and sounds: done / all of the running loads
    pub done: usize,
    pub total: usize,

    jobs: u32,
}

pub type SceneLoadingStatusItem = Arc<RwLock<SceneLoadingStatus>>;

impl SceneLoadingStatus
{
    pub fn is_loading(&self) -> bool
    {
        self.jobs > 0
    }

    pub fn progress(&self) -> f32
    {
        if self.total == 0 { 0.0 } else { (self.done as f32 / self.total as f32).min(1.0) }
    }

    // a new load: starts from zero unless another one is still running
    pub(crate) fn begin_job(&mut self, scenes: &[(u32, String)], total: usize)
    {
        if self.jobs == 0
        {
            *self = SceneLoadingStatus::default();
        }

        self.jobs += 1;
        self.scenes.extend(scenes.iter().cloned());
        self.total += total;
    }

    // a scene file read in the thread brings its count later
    pub(crate) fn add_total(&mut self, total: usize)
    {
        self.total += total;
    }

    pub(crate) fn begin_item(&mut self, item: &str)
    {
        self.current = item.to_string();
    }

    pub(crate) fn finish_item(&mut self, item: Option<&str>)
    {
        self.done += 1;
        if let Some(item) = item
        {
            self.loaded.push(item.to_string());
        }
    }

    pub(crate) fn end_job(&mut self, scene_ids: &[u32])
    {
        self.jobs = self.jobs.saturating_sub(1);
        self.scenes.retain(|(id, _)| !scene_ids.contains(id));
        if self.jobs == 0
        {
            self.current.clear();
        }
    }
}

/// Resets the shared "is loading" flag when the loading task ends (also on early return/panic).
pub struct LoadingGuard(pub Arc<RwLock<bool>>);

impl Drop for LoadingGuard
{
    fn drop(&mut self)
    {
        *self.0.write().unwrap() = false;
    }
}