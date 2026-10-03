use std::fs::File;
use serde::{Deserialize, Serialize};

const EDITOR_SETTINGS_FILE: &str = "data/editor_settings.json";

#[derive(Serialize, Deserialize)]
pub struct EditorSettings
{
    pub load_last_recent: bool
}

impl EditorSettings
{
    pub fn new() -> Self
    {
        let mut settings = Self
        {
            load_last_recent: false
        };

        settings.load();
        settings
    }

    pub fn load(&mut self)
    {
        if let Ok(file) = File::open(EDITOR_SETTINGS_FILE)
        {
            if let Ok(data) = serde_json::from_reader(file)
            {
                *self = data;
            }
        }
    }

    pub fn save(&mut self)
    {
        if let Ok(file) = File::create(EDITOR_SETTINGS_FILE)
        {
            let _ = serde_json::to_writer_pretty(file, self);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.checkbox(&mut self.load_last_recent, "Load last recent project on startup");

        if ui.button("Save settings").clicked()
        {
            self.save();
        }
    }
}