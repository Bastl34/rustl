use std::sync::{Arc, RwLock};

use rfd::FileDialog;

use crate::{helper::concurrency::execution_queue::ExecutionQueue, state::scene::{components::material::TextureType, utilities::scene_utils::load_texture}};

pub fn load_texture_dialog(main_queue: Arc<RwLock<ExecutionQueue>>, texture_type: TextureType, scene_id: u64, material_id: Option<u64>, mipmapping: bool)
{
    if let Some(path) = FileDialog::new().add_filter("Image", &["jpg", "png"]).set_directory("/").pick_file()
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
        load_texture(path.as_str(), main_queue, texture_type, scene_id, material_id, mipmapping);
    }
}