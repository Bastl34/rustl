use std::mem::swap;

use egui::{Ui, RichText, Color32};

use crate::{component_downcast, gui::{editor::editor_state::EditorState, helper::generic_items::{self, collapse_with_title, label_with_background}}, helper::concurrency::thread::spawn_thread, state::{scene::{components::{material::{Material, TextureType}, mesh::Mesh}, scene::Scene}, state::State}};

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

    // Extras
    collapse_with_title(ui, "scene_extras", true, "⊞ Extras", None, |ui|
    {
        ui.scope(|ui|
        {
            for (key, value) in scene.extras.iter()
            {
                ui.label(format!("⚫ {}: {:?}", key, value));
            }
        });
    });

    // Tags
    collapse_with_title(ui, "scene_tags", true, "🔖 Tags", None, |ui|
    {
        ui.scope(|ui|
        {
            ui.vertical( |ui|
            {
                let mut delete_tag = "".to_string();

                // list all tags
                {
                    for (tag, data) in scene.tags.iter()
                    {
                        ui.horizontal(|ui|
                        {
                            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

                            let color_u8 = Color32::from_rgb((data.color.x * 255.0) as u8, (data.color.y * 255.0) as u8,(data.color.z * 255.0) as u8);
                            label_with_background(ui, tag, color_u8, None);

                            ui.add_enabled_ui(!data.locked, |ui|
                            {
                                let hover_text = if data.locked { "locked - can not be deleted via ui" } else { "delete tag" };

                                if ui.button(RichText::new("✖").size(16.0).color(Color32::WHITE)).on_hover_text(hover_text).clicked()
                                {
                                    delete_tag = tag.clone();
                                }
                            });
                        });
                    }
                }

                // delete tag
                if delete_tag.len() > 0
                {
                    scene.tags.remove(delete_tag.as_str());
                }

                // add new tag
                ui.horizontal(|ui|
                {
                    ui.spacing_mut().item_spacing.x = 2.0;

                    ui.set_max_width(150.0);
                    ui.text_edit_singleline(&mut editor_state.tag_input);
                    if ui.button(RichText::new("➕").size(16.0).color(Color32::WHITE)).clicked()
                    {
                        if !editor_state.tag_input.is_empty()
                        {
                            scene.tags.insert(editor_state.tag_input.as_str());
                            editor_state.tag_input.clear();
                        }
                    }
                });
            });
        });
    });

    // Settings
    collapse_with_title(ui, "scene_settings", true, "⛭ Scene Settings", None, |ui|
    {
        scene.ui(ui);
    });

    // Physics
    collapse_with_title(ui, "scene_physics", true, "🌍 Physics Settings", None, |ui|
    {
        {
            let physics = &mut scene.physics.settings;

            ui.horizontal(|ui|
            {
                ui.label("gravity: ");
                ui.add(egui::DragValue::new(&mut physics.gravity.x).speed(0.1).prefix("x: "));
                ui.add(egui::DragValue::new(&mut physics.gravity.y).speed(0.1).prefix("y: "));
                ui.add(egui::DragValue::new(&mut physics.gravity.z).speed(0.1).prefix("z: "));
            });

            ui.separator();

            // the solver needs a constant step, so this is shown as the rate it runs at
            ui.horizontal(|ui|
            {
                ui.label("solver rate: ");

                let mut rate = (1.0 / physics.fixed_timestep).round();

                if ui.add(egui::Slider::new(&mut rate, 30.0..=240.0).fixed_decimals(0).suffix(" Hz")).changed()
                {
                    physics.fixed_timestep = 1.0 / rate.max(1.0);
                }

                ui.label("ℹ").on_hover_text("the solver needs a constant step, the frame time is not - a higher rate is more stable and more expensive");
            });

            ui.horizontal(|ui|
            {
                ui.label("solver iterations: ");
                ui.add(egui::Slider::new(&mut physics.solver_iterations, 1..=32));
                ui.label("ℹ").on_hover_text("how hard the solver works per step. More is steadier for piles and stacks, and more expensive. It does not cure an unstable contact: a rack of nudged bowling pins gets worse at 32, not better");
            });

            ui.horizontal(|ui|
            {
                ui.label("max substeps: ");
                ui.add(egui::Slider::new(&mut physics.max_substeps, 1..=16));
                ui.label("ℹ").on_hover_text("upper limit of solver steps per frame, so a long hitch does not turn into a burst of catch up steps");
            });

            ui.separator();

            ui.label("sleeping");

            ui.horizontal(|ui|
            {
                ui.label("linear threshold: ");
                ui.add(egui::Slider::new(&mut physics.sleep_linear_threshold, 0.0..=1.0).fixed_decimals(3));
                ui.label("ℹ").on_hover_text("a body sleeps once it stays below this speed and the angular threshold long enough - raise it to settle things sooner, at the price of freezing genuinely slow motion. in world units per second, so this assumes meters");
            });

            ui.horizontal(|ui|
            {
                ui.label("angular threshold: ");
                ui.add(egui::Slider::new(&mut physics.sleep_angular_threshold, 0.0..=5.0).fixed_decimals(3)).on_hover_text("in radians per second");
            });

            ui.horizontal(|ui|
            {
                ui.label("time until sleep: ");
                ui.add(egui::Slider::new(&mut physics.time_until_sleep, 0.0..=5.0).fixed_decimals(2).suffix(" s"));
            });

            ui.separator();

            ui.label("reacting on a hit");

            ui.horizontal(|ui|
            {
                ui.label("hit speed: ");
                ui.add(egui::Slider::new(&mut physics.hit_speed, 0.0..=10.0).fixed_decimals(2));
                ui.label("ℹ").on_hover_text("an object that reacts on its first hit ignores anything that touches it slower than this, in units per second. Resting weight never counts; 1.0 is a drop from about 5 cm. The character always counts, a moving kinematic body too.");
            });

            ui.separator();

            ui.checkbox(&mut physics.ground_plane, "Endless Ground Plane").on_hover_text("a floor everything can always land on - the editor grid is only a visual helper and is rebuilt whenever the grid settings change, so it cannot be used for this");

            ui.add_enabled_ui(physics.ground_plane, |ui|
            {
                ui.horizontal(|ui|
                {
                    ui.label("ground plane y: ");
                    ui.add(egui::DragValue::new(&mut physics.ground_plane_y).speed(0.1));
                });
            });

        }

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label(format!("colliders: {} / bodies: {} / combined: {} / waiting: {}", scene.physics.collider_amount(), scene.physics.body_amount(), scene.physics.combined_amount(), scene.physics.waiting_amount()));
            ui.label("ℹ").on_hover_text(format!("synced last frame: {} / shape rebuilds: {}
both should be 0 while nothing but the character moves
combined: objects whose meshes share one body
waiting: objects holding still for their first hit", scene.physics.last_synced, scene.physics.last_shape_rebuilds));

            if ui.button("Rebuild").clicked()
            {
                scene.build_physics();
            }
        });
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