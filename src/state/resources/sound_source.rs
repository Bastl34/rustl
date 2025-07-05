#![allow(dead_code)]

use std::{fs, io::Cursor, sync::{Arc, RwLock}};

use serde::{Deserialize, Serialize};

use crate::{helper::{self, asset_path_descriptor::AssetPathDesciptor}, output::audio_device::AudioDeviceItem, state::scene::manager::id_manager};

pub type SoundSourceItem = Arc<RwLock<Box<SoundSource>>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct SoundSource
{
    pub id: u64,
    pub uuid: String,
    pub source: Option<AssetPathDesciptor>,

    pub name: String,
    pub extension: Option<String>,
    pub hash: String, // this is mainly used for initial loading and to check if there is a sound already loaded (in dynamic textires - this may does not get updates)

    #[serde(skip, default)]
    pub bytes: Arc<Vec<u8>>,

    #[serde(skip, default)]
    pub audio_device: AudioDeviceItem,
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
    type DecoderItem: rodio::Sample + Send + Sync;
    type Decoder: rodio::Source + Send + Iterator<Item = Self::DecoderItem>;

    fn decoder(&self) -> Self::Decoder;
}

impl Decodable for SoundSource
{
    type DecoderItem = <rodio::Decoder<Cursor<SoundSource>> as Iterator>::Item;
    type Decoder = rodio::Decoder<Cursor<SoundSource>>;

    fn decoder(&self) -> Self::Decoder
    {
        let decoder = rodio::Decoder::new(Cursor::new(self.clone())).unwrap();
        decoder
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

            audio_device,

            bytes: Arc::new(bytes),
        }
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

        ui.label(format!("Format: {}", extension));
        ui.label(format!("Size {:.2} MB", sound_size));
    }

    pub fn ui(&mut self, _ui: &mut egui::Ui)
    {

    }
}