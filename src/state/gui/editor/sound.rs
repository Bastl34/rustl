use std::collections::HashMap;

use egui::{Color32, RichText, Ui};
use rfd::FileDialog;

use crate::{helper::concurrency::thread::spawn_thread, state::{gui::helper::{generic_items::collapse_with_title, info_box::info_box}, scene::{components::sound::Sound, scene::Scene, sound_source::SoundSourceItem}, state::State}};

use super::{dialogs::load_sound_dialog, editor_state::{EditorState, SelectionType, SettingsPanel}};

pub fn build_sound_sources_list(editor_state: &mut EditorState, sound_sources: &HashMap<std::string::String, SoundSourceItem>, ui: &mut Ui, scene_id: u64)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (_sound_hash, sound) in sound_sources
        {
            let sound = sound.read().unwrap();
            let headline_name = format!("⚫ {}: {}", sound.id, sound.as_ref().name);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !filter.is_empty() && sound.as_ref().name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let id = format!("soundsource_{}", sound.id);

            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::SoundSource && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if selection
                {

                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = Some(scene_id);
                    editor_state.selected_type = SelectionType::SoundSource;
                    editor_state.settings = SettingsPanel::SoundSource;
                }
                else
                {
                    editor_state.selected_object.clear();
                    editor_state.selected_scene_id = None;
                }
            }
        }
    });
}

pub fn create_sound_source_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u64 = editor_state.selected_scene_id.unwrap();

    let (sound_source_id, ..) = editor_state.get_object_ids();

    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if sound_source_id.is_none() { return; }
    let sound_source_id = sound_source_id.unwrap();

    if let Some(sound_source) = scene.get_sound_source_by_id(sound_source_id)
    {
        collapse_with_title(ui, "sound_source_info", true, "🔊 Sound Info", None, |ui|
        {
            {
                let sound_source = sound_source.write().unwrap();
                sound_source.ui_info(ui);
            }
        });

        collapse_with_title(ui, "sound_source_settings", true, "🔊 Sound Source Settings", None, |ui|
        {
            let mut changed = false;

            let mut name;
            {
                let sound_source = sound_source.read().unwrap();

                name = sound_source.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });

            if changed
            {
                let mut sound_source = sound_source.write().unwrap();

                sound_source.name = name;
            }

            {
                let mut sound_source = sound_source.write().unwrap();
                sound_source.ui(ui);
            }
        });

        collapse_with_title(ui, "sound_source_usage", true, "👆 Sound used by Components", None, |ui|
        {
            let mut used = false;

            let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

            for node in all_nodes
            {
                for component in node.read().unwrap().find_components::<Sound>()
                {
                    let component = component.read().unwrap();
                    let component_id = component.id();

                    ui.horizontal(|ui|
                    {
                        ui.label(format!(" ⚫ {}: {}", component_id, component.get_base().name));

                        // link to the material setting
                        if ui.button(RichText::new("⮊").color(Color32::WHITE)).on_hover_text("go to sound").clicked()
                        {
                            editor_state.selected_object = format!("sound_{}", component_id);
                            editor_state.selected_scene_id = Some(scene_id);
                            editor_state.selected_type = SelectionType::Sound;
                            editor_state.settings = SettingsPanel::Sound;
                        }
                    });

                    used = true;
                }
            }

            if !used
            {
                info_box(ui, "This sound is not used by any component. Try removing it to save resources.");
            }
        });

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("💾 Save Sound Source").heading().strong().color(Color32::WHITE)).clicked()
            {
                let sound_source = sound_source.read().unwrap();
                let extension = sound_source.extension.clone().unwrap_or("unkown".to_string());

                let name = format!("{}.{}", sound_source.name.clone(), extension.clone());
                if let Some(path) = FileDialog::new().add_filter("Sound", &[extension]).set_directory("/").set_file_name(name).save_file()
                {
                    sound_source.save(path.into_os_string().to_str().unwrap());
                }
            }
        });

        // delete sound Source
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Sound Source").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                scene.delete_sound_source_by_id(sound_source_id);
            }
        });
    }
}

pub fn create_sound_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // no scene selected
    if editor_state.selected_scene_id.is_none() { return; }
    let scene_id: u64 = editor_state.selected_scene_id.unwrap();

    let (sound_id, ..) = editor_state.get_object_ids();

    let main_queue = state.main_thread_execution_queue.clone();
    let scene = state.find_scene_by_id_mut(scene_id);
    if scene.is_none() { return; }

    let scene = scene.unwrap();

    if sound_id.is_none() { return; }
    let sound_id = sound_id.unwrap();

    if let Some(sound) = scene.get_sound_by_id(sound_id)
    {
        collapse_with_title(ui, "sound_settings", true, "🔊 Sound Settings", None, |ui|
        {
            let mut sound = sound.write().unwrap();
            sound.ui(ui, None);
        });

        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Load Sound").heading().strong()).clicked()
            {
                let main_queue = main_queue.clone();
                spawn_thread(move ||
                {
                    load_sound_dialog(main_queue.clone(), scene_id, Some(sound_id));
                });
            }
        });
    }
}