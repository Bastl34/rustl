#![allow(dead_code)]

use std::{fs, io::Cursor, sync::{Arc, RwLock}};

use serde::{Deserialize, Serialize};

use crate::{helper::{self, asset_path_descriptor::AssetPathDesciptor, file::{get_extension, get_stem}}, output::audio_device::AudioDeviceItem, resources::resources::{load_binary, to_resource_path}, state::scene::{manager::id_manager, utilities::tags::Tags}};

pub type SoundSourceItem = Arc<RwLock<Box<SoundSource>>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct SoundSource
{
    #[serde(skip, default)]
    pub id: u32,

    pub uuid: String,
    pub source: Option<AssetPathDesciptor>,

    pub name: String,
    pub extension: Option<String>,
    pub hash: String, // this is mainly used for initial loading and to check if there is a sound already loaded (in dynamic textires - this may does not get updates)
    pub tags: Tags,

    #[serde(skip, default)]
    pub bytes: Arc<Vec<u8>>,

    #[serde(skip, default)]
    pub audio_device: AudioDeviceItem,

    #[serde(skip, default)]
    pub delete_later_request: bool,

    // uuids other scenes gave the same file - they resolve to this one
    #[serde(skip, default)]
    pub uuid_aliases: Vec<String>,
}

impl AsRef<[u8]> for SoundSource
{
    fn as_ref(&self) -> &[u8]
    {
        &self.bytes
    }
}

pub trait Decodable: Send + Sync + 'static
{
    type Decoder: rodio::Source<Item = f32> + Send;

    fn decoder(&self) -> Option<Self::Decoder>;
}

impl Decodable for SoundSource
{
    type Decoder = rodio::Decoder<Cursor<SoundSource>>;

    fn decoder(&self) -> Option<Self::Decoder>
    {
        rodio::Decoder::try_from(Cursor::new(self.clone())).ok()
    }
}

impl SoundSource
{
    pub fn new(name: &str, audio_device: AudioDeviceItem, sound_bytes: &Vec<u8>, extension: Option<String>) -> SoundSource
    {
        let bytes = sound_bytes.clone();
        let hash = helper::crypto::get_hash_from_byte_vec(sound_bytes);

        SoundSource
        {
            id: id_manager::get_next_sound_source_id(),
            uuid: uuid::Uuid::new_v4().to_string(),
            source: None,

            name: name.to_string(),
            extension,
            hash,
            tags: Tags::new(),

            audio_device,

            bytes: Arc::new(bytes),

            delete_later_request: false,
            uuid_aliases: vec![],
        }
    }

    // a sound file: bundled ones by their path inside the resources, the uuid follows from the path, so every scene gives a file the same one
    pub fn from_path(path: &str, audio_device: AudioDeviceItem) -> anyhow::Result<SoundSource>
    {
        let path = canonical_sound_path(path);
        let bytes = load_binary(&path)?;

        Ok(Self::from_file_bytes(&path, &bytes, audio_device))
    }

    // the bytes were read from "path" already (off the main thread)
    pub fn from_file_bytes(path: &str, bytes: &Vec<u8>, audio_device: AudioDeviceItem) -> SoundSource
    {
        let path = canonical_sound_path(path);

        let mut sound_source = SoundSource::new(&get_stem(&path), audio_device, bytes, Some(get_extension(&path).to_lowercase()));
        sound_source.uuid = path_uuid(&path);
        sound_source.source = Some(AssetPathDesciptor::new_from_path(path));

        sound_source
    }

    pub fn matches_uuid(&self, uuid: &str) -> bool
    {
        self.uuid == uuid || self.uuid_aliases.iter().any(|alias| alias == uuid)
    }

    pub fn origin_path(&self) -> Option<&str>
    {
        self.source.as_ref().map(|source| source.origin_path.as_str())
    }

    pub fn delete_later(&mut self)
    {
        self.delete_later_request = true;
    }

    pub fn save(&self, path: &str) -> bool
    {
        let res = fs::write(path, self.bytes.as_slice());
        res.is_ok()
    }

    pub fn ui_info(&self, ui: &mut egui::Ui)
    {
        let sound_size = self.bytes.len() as f32 / 1024.0 / 1024.0;
        let extension = self.extension.clone().unwrap_or("unknown".to_string());

        ui.label(format!("Path: {}", self.origin_path().unwrap_or("-")));
        ui.label(format!("Hash: {}", self.hash));

        ui.label(format!("Format: {}", extension));
        ui.label(format!("Size {:.2} MB", sound_size));
    }

    pub fn ui(&mut self, _ui: &mut egui::Ui)
    {

    }
}

// bundled files relative to the resources ("sounds/vehicle/skid.ogg"), other files as they are
pub fn canonical_sound_path(path: &str) -> String
{
    to_resource_path(path).unwrap_or_else(|| path.replace('\\', "/"))
}

pub fn path_uuid(path: &str) -> String
{
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, format!("rustl/sound/{}", canonical_sound_path(path)).as_bytes()).to_string()
}

pub fn find_sound_source(sources: &[SoundSourceItem], uuid: &str) -> Option<SoundSourceItem>
{
    sources.iter().find(|source| source.read().unwrap().matches_uuid(uuid)).cloned()
}
