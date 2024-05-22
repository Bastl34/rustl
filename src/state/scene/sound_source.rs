use std::{fs, io::Cursor, sync::{Arc, RwLock}};

use crate::{helper::{self}, output::audio_device::AudioDeviceItem};

pub type SoundSourceItem = Arc<RwLock<Box<SoundSource>>>;

#[derive(Clone)]
pub struct SoundSource
{
    pub id: u64,
    pub name: String,
    pub extension: Option<String>,
    pub hash: String, // this is mainly used for initial loading and to check if there is a sound already loaded (in dynamic textires - this may does not get updates)

    pub bytes: Arc<Vec<u8>>,

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

impl Drop for SoundSource
{
    fn drop(&mut self)
    {
        dbg!("droppinggggggggggggggggggggggg SoundSource", self.name.clone());
    }
}

impl SoundSource
{
    pub fn new(id: u64, name: &str, audio_device: AudioDeviceItem, sound_bytes: &Vec<u8>, extension: Option<String>) -> SoundSource
    {
        let bytes = sound_bytes.clone();
        let hash = helper::crypto::get_hash_from_byte_vec(sound_bytes);

        SoundSource
        {
            id,
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

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {

    }
}