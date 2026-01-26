use std::sync::{Arc, RwLock};

use rfd::FileDialog;

use crate::{helper::concurrency::execution_queue::ExecutionQueue, state::scene::{components::material::TextureType, utilities::scene_utils::{load_sound, load_texture}}};

pub fn load_texture_dialog(main_queue: Arc<RwLock<ExecutionQueue>>, texture_type: Option<TextureType>, scene_id: Option<u32>, material_id: Option<u32>, mipmapping: bool, max_tex_res: u32)
{
    if let Some(path) = FileDialog::new().add_filter("Image", &["jpg", "png", "webp"]).set_directory("/").pick_file()
    {
        let name: Option<&std::ffi::OsStr> = path.file_stem().clone();
        let extension = path.extension().clone();

        if name.is_none() ||  name.unwrap().to_str().is_none()
        {
            return;
        }

        if extension.is_none() ||  extension.unwrap().to_str().is_none()
        {
            return;
        }

        let path = &path.display().to_string();
        load_texture(path.as_str(), main_queue, texture_type, scene_id, material_id, mipmapping, max_tex_res);
    }
}

pub fn load_sound_dialog(main_queue: Arc<RwLock<ExecutionQueue>>, sound_component_id: Option<u32>)
{
    if let Some(path) = FileDialog::new().add_filter("Audio", &["ogg", "mp3", "wav", "flac"]).set_directory("/").pick_file()
    {
        let name: Option<&std::ffi::OsStr> = path.file_stem().clone();
        let extension = path.extension().clone();

        if name.is_none() ||  name.unwrap().to_str().is_none()
        {
            return;
        }

        if extension.is_none() ||  extension.unwrap().to_str().is_none()
        {
            return;
        }

        let path = &path.display().to_string();
        load_sound(path.as_str(), main_queue, sound_component_id);
    }
}