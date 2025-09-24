use std::mem::swap;

use egui::{Ui, RichText, Color32};

use crate::{component_downcast, helper::concurrency::thread::spawn_thread, state::{gui::{editor::editor_state::EditorState, helper::generic_items::{self, collapse_with_title}}, scene::{components::{material::{Material, TextureType}, mesh::Mesh}, scene::Scene}, state::State}};

use super::dialogs::load_texture_dialog;

pub fn create_scene_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let scene_id = editor_state.selected_scene_id;

    // no scene selected
    if scene_id.is_none()
    {
        return;
    }

    let main_queue = state.main_thread_execution_queue.clone();

    let scene_id = scene_id.unwrap();
    let max_tex_res = state.max_texture_resolution();
    let scene = state.find_scene_by_id_mut(scene_id);

    if scene.is_none()
    {
        return;
    }

    let scene = scene.unwrap();

    let mut instances_amout = 0;
    let mut meshes_amout = 0;
    let mut nodes_solid_amout = 0;
    let mut nodes_transparent_amout = 0;

    let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

    for node in &all_nodes
    {
        let node = node.read().unwrap();
        instances_amout += node.instances.get_ref().len();

        let mesh = node.find_component::<Mesh>();
        if mesh.is_some()
        {
            meshes_amout += 1;
        }

        if let Some(material) = node.find_component::<Material>()
        {
            component_downcast!(material, Material);
            if material.has_transparency()
            {
                nodes_transparent_amout += 1;
            }
            else
            {
                nodes_solid_amout += 1;
            }
        }
    }

    // statistics
    collapse_with_title(ui, "scene_info", true, "📈 Info", None, |ui|
    {
        ui.label(RichText::new("🎬 scene").strong());
        ui.label(format!(" ⚫ nodes: {}", all_nodes.len()));

        ui.horizontal(|ui|
        {
            ui.add_space(16.0);
            ui.vertical(|ui|
            {
                ui.label(format!(" ⚫ solid: {}", nodes_solid_amout));
                ui.label(format!(" ⚫ transparent: {}", nodes_transparent_amout));
            });
        });
        ui.label(format!(" ⚫ instances: {}", instances_amout));
        ui.label(format!(" ⚫ materials: {}", scene.materials.len()));
        ui.label(format!(" ⚫ cameras: {}", scene.cameras.len()));
        ui.label(format!(" ⚫ lights: {}", scene.lights.get_ref().len()));

        ui.label(RichText::new("◼ geometry").strong());
        ui.label(format!(" ⚫ meshes: {}", meshes_amout));
    });

    // Settings
    collapse_with_title(ui, "scene_settings", true, "⛭ Scene Settings", None, |ui|
    {
        scene.ui(ui);
    });

    // Env Texture
    if let Some(texture) = scene.get_data().environment_texture.clone()
    {
        let mut enabled = texture.enabled;
        let texture = texture.get();

        if let Some(texture) = texture
        {
            let mut texture = texture.write().unwrap();

            let title = format!("🖼 {} Texture", TextureType::Environment.to_string());
            let id = format!("texture_{}", TextureType::Environment.to_string());

            let mut remove_texture = false;
            let mut changed = false;

            generic_items::collapse(ui, id, true, None, |ui|
            {
                ui.label(RichText::new(title).heading().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                {
                    if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                    {
                        remove_texture = true;
                    }

                    // enabled toggle
                    let toggle_text;
                    if enabled
                    {
                        toggle_text = RichText::new("⏺").color(Color32::GREEN);
                    }
                    else
                    {
                        toggle_text = RichText::new("⏺").color(Color32::RED);
                    }


                    if ui.toggle_value(&mut enabled, toggle_text).clicked()
                    {
                        changed = true;
                    }
                });
            },
            |ui|
            {
                texture.ui_info(ui);
            });

            if changed
            {
                let scene_data = scene.get_data_mut();
                let scene_data = scene_data.get_mut();
                let env_tex = scene_data.environment_texture.as_mut().unwrap();
                env_tex.enabled = enabled;
            }

            if remove_texture
            {
                let scene_data = scene.get_data_mut();
                let scene_data = scene_data.get_mut();
                scene_data.environment_texture = None;
            }
        }
    }
    else
    {
        let title = format!("🖼 {} Texture", TextureType::Environment.to_string());
        let id = format!("texture_{}", TextureType::Environment.to_string());

        generic_items::collapse(ui, id, true, None, |ui|
        {
            ui.label(RichText::new(title).heading().strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                // "enabled" toggle
                let toggle_text = RichText::new("⏺").color(Color32::RED);

                ui.add_enabled_ui(false, |ui|
                {
                    let mut enabled = false;
                    ui.toggle_value(&mut enabled, toggle_text)
                });
            });
        },
        |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
            {
                if ui.button(RichText::new("Load Texture").heading().strong()).clicked()
                {
                    spawn_thread(move ||
                    {
                        load_texture_dialog(main_queue.clone(), Some(TextureType::Environment), Some(scene_id), None, true, max_tex_res);
                    });
                }
            });
        });
    }

    // Debugging
    collapse_with_title(ui, "scene_debugging", true, "🐛 Debugging Settings", None, |ui|
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            if ui.button("save image").clicked()
            {
                state.save_image = true;
            }

            if ui.button("save depth pass image").clicked()
            {
                state.save_depth_pass_image = true;
            }

            if ui.button("save depth buffer image").clicked()
            {
                state.save_depth_buffer_image = true;
            }

            if ui.button("save screenshot").clicked()
            {
                state.save_screenshot = true;
            }
        });
    });


    // Pre Scene Controller
    {
        ui.separator();
        ui.label(RichText::new("Pre Scene Controller").heading().strong());

        let scene = state.find_scene_by_id_mut(scene_id).unwrap();
        let mut controller = vec![];
        swap(&mut scene.pre_controller, &mut controller);

        let mut delete_controller = None;

        for (i, controller) in controller.iter_mut().enumerate()
        {
            let mut enabled;
            let name;
            {
                enabled = controller.get_base().is_enabled;
                name = format!("{} {}",controller.get_base().icon.clone(), controller.get_base().name.clone());
            }

            generic_items::collapse(ui, format!("pre_scene_controller_{}", i).to_string(), true, None, |ui|
            {
                ui.label(RichText::new(name).heading().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                {
                    if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                    {
                        delete_controller = Some(i);
                    }

                    // enabled toggle

                    let toggle_text;
                    if enabled
                    {
                        toggle_text = RichText::new("⏺").color(Color32::GREEN);
                    }
                    else
                    {
                        toggle_text = RichText::new("⏺").color(Color32::RED);
                    }

                    ui.toggle_value(&mut enabled, toggle_text)
                });
            },
            |ui|
            {
                controller.ui(ui, scene);
            });

            controller.get_base_mut().is_enabled = enabled;
        }

        // swap back
        swap(&mut controller, &mut scene.pre_controller);

        if let Some(delete_controller) = delete_controller
        {
            if let Some(controller) = scene.pre_controller.get_mut(delete_controller)
            {
                controller.cleanup();
            }
            scene.pre_controller.remove(delete_controller);
        }

        // add scene controller
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Add Controller").heading().strong().color(Color32::WHITE)).clicked()
            {
                editor_state.dialog_add_scene_controller = true;
                editor_state.add_scene_controller_post = false;
            }
        });
    }

    // Post Scene Controller
    {
        ui.separator();
        ui.label(RichText::new("Post Scene Controller").heading().strong());

        let scene = state.find_scene_by_id_mut(scene_id).unwrap();
        let mut controller = vec![];
        swap(&mut scene.post_controller, &mut controller);

        let mut delete_controller = None;

        for (i, controller) in controller.iter_mut().enumerate()
        {
            let mut enabled;
            let name;
            {
                enabled = controller.get_base().is_enabled;
                name = format!("{} {}",controller.get_base().icon.clone(), controller.get_base().name.clone());
            }

            generic_items::collapse(ui, format!("post_scene_controller_{}", i).to_string(), true, None, |ui|
            {
                ui.label(RichText::new(name).heading().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                {
                    if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                    {
                        delete_controller = Some(i);
                    }

                    // enabled toggle

                    let toggle_text;
                    if enabled
                    {
                        toggle_text = RichText::new("⏺").color(Color32::GREEN);
                    }
                    else
                    {
                        toggle_text = RichText::new("⏺").color(Color32::RED);
                    }

                    ui.toggle_value(&mut enabled, toggle_text)
                });
            },
            |ui|
            {
                controller.ui(ui, scene);
            });

            controller.get_base_mut().is_enabled = enabled;
        }

        // swap back
        swap(&mut controller, &mut scene.post_controller);

        if let Some(delete_controller) = delete_controller
        {
            if let Some(controller) = scene.post_controller.get_mut(delete_controller)
            {
                controller.cleanup();
            }
            scene.post_controller.remove(delete_controller);
        }

        // add scene controller
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui|
        {
            if ui.button(RichText::new("Add Controller").heading().strong().color(Color32::WHITE)).clicked()
            {
                editor_state.dialog_add_scene_controller = true;
                editor_state.add_scene_controller_post = false;
            }
        });
    }
}