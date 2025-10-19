use std::{collections::HashMap, path::Path};

use egui::{RichText, Ui};

use crate::{component_downcast, helper::generic::cut_string_to_length, state::{gui::{editor::editor::{EDITOR_INTERNAL_TAG, MAX_NAME_LENGTH}, helper::{generic_items::collapse_with_title, info_box::info_box}}, resources::mesh_resource::MeshResourceItem, scene::{components::{component::Component, mesh::Mesh}, scene::Scene}, state::{State, ENGINE_INTERNAL_TAG}}};

use super::super::editor_state::{EditorState, SelectionType, SettingsPanel};

pub fn build_mesh_resources_list(editor_state: &mut EditorState, mesh_resources: &HashMap<std::string::String, MeshResourceItem>, ui: &mut Ui)
{
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
    {
        for (_mesh_hash, mesh) in mesh_resources
        {
            let mesh: std::sync::RwLockReadGuard<'_, Box<crate::state::resources::mesh_resource::MeshResource>> = mesh.read().unwrap();
            let mut mesh_resource_name = mesh.name.clone();
            if let Some(mesh_source) = mesh.source.as_ref()
            {
                let path = Path::new(&mesh_source.origin_path);
                let filename = path.file_name().unwrap();
                mesh_resource_name += format!(" ({})", filename.to_string_lossy()).as_str();
            }

            let headline_name = format!("⚫ {}: {}", mesh.id, cut_string_to_length(&mesh_resource_name, MAX_NAME_LENGTH));

            let is_internal = mesh.tags.contains(ENGINE_INTERNAL_TAG) || mesh.tags.contains(EDITOR_INTERNAL_TAG);
            let show_from_tags = !is_internal || (is_internal && editor_state.show_internal_entries);

            let filter = editor_state.hierarchy_filter.to_lowercase();
            if !show_from_tags || !filter.is_empty() && mesh.as_ref().name.to_lowercase().find(filter.as_str()).is_none()
            {
                continue;
            }

            let id = format!("mesh-resource_{}", mesh.id);

            let heading = RichText::new(headline_name).strong();

            let mut selection; if editor_state.selected_type == SelectionType::MeshResource && editor_state.selected_object == id { selection = true; } else { selection = false; }
            if ui.toggle_value(&mut selection, heading).clicked()
            {
                if selection
                {

                    editor_state.selected_object = id;
                    editor_state.selected_scene_id = None;
                    editor_state.selected_type = SelectionType::MeshResource;
                    editor_state.settings = SettingsPanel::MeshResource;
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

pub fn create_mesh_resource_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let (mesh_resource_id, ..) = editor_state.get_object_ids();

    if mesh_resource_id.is_none() { return; }
    let mesh_resource_id = mesh_resource_id.unwrap();

    if let Some(mesh_resource) = state.get_mesh_resource_by_id(mesh_resource_id)
    {
        collapse_with_title(ui, "mesh_resource_info", true, "🔷 Mesh Info", None, |ui|
        {
            {
                let mesh_resource = mesh_resource.read().unwrap();
                mesh_resource.ui_info(ui);
            }
        });

        collapse_with_title(ui, "mesh_resource_settings", true, "🔷 Meh Source Settings", None, |ui|
        {
            let mut changed = false;

            let mut name;
            {
                let mesh_resource = mesh_resource.read().unwrap();

                name = mesh_resource.name.clone();
            }

            ui.horizontal(|ui|
            {
                ui.label("name: ");
                changed = ui.text_edit_singleline(&mut name).changed() || changed;
            });

            if changed
            {
                let mut mesh_resource = mesh_resource.write().unwrap();

                mesh_resource.name = name;
            }

            /*
            {
                let mut mesh_resource = mesh_resource.write().unwrap();
                mesh_resource.ui(ui);
            }
            */
        });

        collapse_with_title(ui, "mesh_resource_usage", true, "👆 Used by Components", None, |ui|
        {
            let mut used = false;

            for scene in &state.scenes
            {
                let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

                for node in all_nodes
                {
                    for mesh in node.read().unwrap().find_components::<Mesh>()
                    {
                        component_downcast!(mesh, Mesh);

                        if let Some(mesh_resource) = mesh.mesh_resource.as_ref()
                        {
                            if mesh_resource_id == mesh_resource.read().unwrap().id
                            {
                                ui.horizontal(|ui|
                                {
                                    ui.label(format!(" ⚫ {}: {}", mesh.id(), mesh.get_base().name));
                                });

                                used = true;
                            }
                        }
                    }
                }
            }

            if !used
            {
                info_box(ui, "This mesh resouce is not used by any component. Try removing it to save resources.");
            }
        });

        // delete Mesh Resource
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Dispose Mesh Resouce").heading().strong().color(ui.visuals().error_fg_color)).clicked()
            {
                state.delete_mesh_resource_by_id(mesh_resource_id);
            }
        });
    }
}