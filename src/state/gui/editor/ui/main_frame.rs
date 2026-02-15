
use std::mem::swap;

use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::console_log;
use crate::state::gui::editor::helper::get_object_and_pointer_world_position;
use crate::state::gui::editor::ui::console::create_console_section;
use crate::state::gui::editor::ui::debug::create_debug_settings;
use crate::state::gui::editor::ui::dialogs::load_texture_dialog;
use crate::state::gui::editor::ui::mesh::{build_mesh_resources_list, create_mesh_resource_settings};
use crate::state::scene::utilities::scene_utils::execute_on_scene_mut;
use crate::state::state::ENGINE_INTERNAL_TAG_PREFX;
use crate::{component_downcast, component_downcast_mut};
use crate::helper::concurrency::execution_queue::ExecutionQueueItem;
use crate::state::gui::helper::generic_items::collapse_with_title;
use crate::state::scene::components::component::ComponentItem;
use crate::state::scene::components::transformation::Transformation;
use crate::state::{gui::editor::editor_state::EditorState, state::State};
use crate::state::gui::editor::editor_state::SettingsPanel;
use crate::state::scene::scene::Scene;
use crate::state::scene::exporter::json;
use egui::{Visuals, Style, ScrollArea, Ui, RichText, Color32};
use web_time::Instant;

use super::assets::create_asset_section;
use super::cameras::{build_camera_list, create_camera_settings};
use super::super::editor_state::{SelectionType, BottomPanel};
use super::lights::{build_light_list, create_light_settings};
use super::materials::{build_material_list, create_material_settings};
use super::modals::create_modals;
use super::objects::{build_objects_list, create_object_settings, create_component_settings};
use super::general::create_general_settings;
use super::scenes::create_scene_settings;
use super::sound::{build_sound_sources_list, create_sound_settings, create_sound_source_settings};
use super::statistics::{create_chart, create_statistic};
use super::textures::{create_texture_settings, build_texture_list};

pub fn create_frame(ctx: &egui::Context, editor_state: &mut EditorState, state: &mut State)
{
    let mut visual = Visuals::dark();
    visual.panel_fill[3] = 253;
    //visual.override_text_color = Some(egui::Color32::WHITE);

    let style = Style
    {
        visuals: visual,
        ..Style::default()
    };

    let loading = *editor_state.loading.read().unwrap();

    let frame = egui::Frame::side_top_panel(&style);

    egui::TopBottomPanel::top("top_panel").frame(frame).show(ctx, |ui|
    {
        ui.horizontal(|ui|
        {
            create_file_menu(editor_state, state, ui);
        });
    });

    //bottom
    egui::TopBottomPanel::bottom("bottom_panel").resizable(true).frame(frame).show(ctx, |ui|
    {
        ui.horizontal(|ui|
        {
            ui.selectable_value(&mut editor_state.bottom, BottomPanel::None, "⏷");
            ui.selectable_value(&mut editor_state.bottom, BottomPanel::Assets, "📦 Assets");

            let console_log_amount = console_log::get_amount();
            let console_errors = console_log::get_error_amount();
            if console_errors > 0
            {
                ui.selectable_value(&mut editor_state.bottom, BottomPanel::Console, egui::RichText::new(format!("📝 Console ({} with Errors)", console_log_amount)).color(egui::Color32::LIGHT_RED)).on_hover_text(format!("there are {} errors in the console log", console_errors));
            }
            else
            {
                ui.selectable_value(&mut editor_state.bottom, BottomPanel::Console, format!("📝 Console ({})", console_log_amount));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                if loading
                {
                    ui.separator();
                    ui.spinner();
                }

                // just refresh if mouse was moved
                if state.io.input_manager.get_pointer_input().velocity.magnitude() > 0.0
                {
                    // just run every 1/10 second to reduce performance overhead (especially for skinned meshes)
                    if editor_state.last_hover_check.elapsed().as_secs_f32() > 0.1
                    {
                        editor_state.last_hover_check = Instant::now();

                        if let Some((object_name, pointer_pos)) =  get_object_and_pointer_world_position(state)
                        {
                            editor_state.last_hover_object = Some(object_name);
                            editor_state.last_hover_pointer_position = Some(pointer_pos);
                        }
                        else
                        {
                            editor_state.last_hover_object = None;
                            editor_state.last_hover_pointer_position = None;
                        }
                    }
                }

                if let Some(last_hover_object) = &editor_state.last_hover_object
                {
                    let pointer_pos = editor_state.last_hover_pointer_position.unwrap();
                    ui.label(RichText::new(format!("{} | x: {:.2}, y: {:.2}, z: {:.2}", last_hover_object, pointer_pos.x, pointer_pos.y, pointer_pos.z)).size(12.0));
                }

                let (_scene, node, instance_id) = editor_state.get_selected_node(state);
                if let Some(node) = node
                {
                    ui.separator();

                    if let Some(instance_id) = instance_id
                    {
                        if let Some(instance) = node.read().unwrap().find_instance_by_id(instance_id)
                        {
                            ui.label(RichText::new(format!("(Instance: {})", instance.read().unwrap().name)));
                        }
                    }

                    ui.label(RichText::new(format!("Selected: {}", node.read().unwrap().name)));
                }
            });
        });
        ui.separator();

        if editor_state.bottom == BottomPanel::Assets
        {
            create_asset_section(editor_state, state, ui);
        }
        else if editor_state.bottom == BottomPanel::Console
        {
            create_console_section(editor_state, state, ui);
        }
    });

    //left
    egui::SidePanel::left("left_panel").frame(frame).show(ctx, |ui|
    {
        ui.set_min_width(300.0);

        //ui.add_enabled_ui(!loading, |ui|
        //{
            create_left_sidebar(editor_state, state, ui);
        //});
    });

    //right
    egui::SidePanel::right("right_panel").frame(frame).show(ctx, |ui|
    {
        ui.set_min_width(300.0);

        //ui.add_enabled_ui(!loading, |ui|
        //{
            create_right_sidebar(editor_state, state, ui);
        //});
    });


    //top
    egui::TopBottomPanel::top("top_panel_main").frame(frame).show(ctx, |ui|
    {
        //ui.add_enabled_ui(!loading, |ui|
        //{
            ui.horizontal(|ui|
            {
                create_tool_menu(editor_state, state, ui);
            });
        //});
    });

    // modals
    create_modals(editor_state, state, ctx);
}

fn create_file_menu(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.menu_button("File", |ui|
    {
        if ui.button("Save Project").clicked()
        {
            json::export(state, (("data/".to_string()) + &editor_state.project_name).as_str());
        }
        if ui.button("Exit").clicked()
        {
            state.exit = true;
        }
    });
}

fn create_tool_menu(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let icon_size = 20.0;

    ui.horizontal(|ui|
    {
        let mut fullscreen = state.rendering.fullscreen.get_ref().clone();
        let mut try_out = editor_state.try_mode;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui|
        {
            create_tool_menu_grid(editor_state, state, ui);
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
        {
            // fullscreen change
            if ui.toggle_value(&mut fullscreen, RichText::new("⛶").size(icon_size)).on_hover_text("fullscreen").changed()
            {
                state.rendering.fullscreen.set(fullscreen);
            }

            // try out mode
            if ui.toggle_value(&mut try_out, RichText::new("🎮").size(icon_size)).on_hover_text("try out").changed()
            {
                editor_state.set_try_mode(state, try_out);
            };

            ui.separator();

            // gizmo
            if ui.toggle_value(&mut editor_state.gizmo_scale, RichText::new("🕂").size(icon_size)).on_hover_text("use scale gizmo").clicked()
            {
                if editor_state.gizmo_scale
                {
                    editor_state.gizmo_position = false;
                    editor_state.gizmo_rotation = false;
                }
            }

            if ui.toggle_value(&mut editor_state.gizmo_rotation, RichText::new("↻").size(icon_size)).on_hover_text("use rotation gizmo").clicked()
            {
                if editor_state.gizmo_rotation
                {
                    editor_state.gizmo_position = false;
                    editor_state.gizmo_scale = false;
                }
            }

            if ui.toggle_value(&mut editor_state.gizmo_position, RichText::new("⬌").size(icon_size)).on_hover_text("use position gizmo").clicked()
            {
                if editor_state.gizmo_position
                {
                    editor_state.gizmo_rotation = false;
                    editor_state.gizmo_scale = false;
                }
            }

            ui.separator();

            if ui.toggle_value(&mut editor_state.use_highlight, RichText::new("🔦").size(icon_size)).on_hover_text("highlight objects in the editor").clicked()
            {
                if !editor_state.use_highlight
                {
                    editor_state.remove_highlight(state);
                }
                else
                {
                    editor_state.apply_highlight(state);
                }
            }

            ui.separator();

            // fly camera
            ui.toggle_value(&mut editor_state.fly_camera, RichText::new("✈").size(icon_size)).on_hover_text("fly camera");

            // selectable
            if ui.toggle_value(&mut editor_state.selectable, RichText::new("🖱").size(icon_size)).on_hover_text("select objects").changed()
            {
                if !editor_state.selectable
                {
                    editor_state.de_select_current_item(state);
                }
            }

            ui.separator();

            // play
            let mut playing = !state.pause;
            ui.toggle_value(&mut playing, RichText::new("⏵").size(icon_size)).on_hover_text("Playing/Pause");
            state.pause = !playing;
        });
    });
}

fn create_tool_menu_grid(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // grid size
    let mut changed = false;
    let mut grid_size = editor_state.grid_size;

    ui.label("▓").on_hover_text("Grid");
    egui::ComboBox::from_label("units").selected_text(format!("{grid_size:?}")).show_ui(ui, |ui|
    {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

        changed = ui.selectable_value(&mut grid_size, 0.05, "0.05").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 0.0625, "0.0625").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 0.1, "0.1").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 0.125, "0.125").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 0.25, "0.25").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 0.5, "0.5").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 1.0, "1.0").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 2.5, "2.5").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 5.0, "5.0").changed() || changed;
        changed = ui.selectable_value(&mut grid_size, 10.0, "10.0").changed() || changed;
    });

    if changed
    {
        editor_state.set_grid_size(grid_size);
    }

    ui.separator();

    ui.checkbox(&mut editor_state.drag_and_drop_grid_only, "Only move on Grid");

    ui.separator();

    ui.horizontal(|ui|
    {
        ui.label("Grid Y: ");

        let mut y = 0.0;
        let mut grid_transform: Option<ComponentItem> = None;

        for scene in &mut state.scenes
        {
            let grid = scene.find_node_by_name("grid");

            if let Some(grid) = grid
            {
                let grid = grid.read().unwrap();
                grid_transform = grid.find_component::<Transformation>();

                if let Some(grid_transform) = &grid_transform
                {
                    component_downcast!(grid_transform, Transformation);
                    y = grid_transform.get_data().position.y;
                }
            }
        }

        if ui.add(egui::DragValue::new(&mut y).speed(0.1).prefix("y: ")).changed()
        {
            if let Some(grid_transform) = &grid_transform
            {
                component_downcast_mut!(grid_transform, Transformation);
                let mut pos = grid_transform.get_data().position;
                pos.y = y;
                grid_transform.set_translation(pos);
            }
        }
    });
}

fn create_left_sidebar(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // statistics
    collapse_with_title(ui, "chart", true, "📈 Chart", None, |ui|
    {
        create_chart(editor_state, state, ui);
    });

    // statistics
    collapse_with_title(ui, "statistic", true, "ℹ Statistics", None, |ui|
    {
        create_statistic(editor_state, state, ui);
    });

    // hierarchy
    collapse_with_title(ui, "hierarchy", true, "🗄 Hierarchy", None, |ui|
    {
        ScrollArea::vertical().show(ui, |ui|
        {
            ui.scope(|ui|
            {
                ui.style_mut().visuals.indent_has_left_vline = true;
                create_hierarchy(editor_state, state, ui);
            });
        });
    });
}

fn create_right_sidebar(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let mut object_settings = false;
    let mut camera_settings = false;
    let mut light_settings = false;
    let mut material_settings = false;
    let mut sound_settings = false;

    let mut texture_settings = false;
    let mut sound_source_settings = false;
    let mut mesh_resource_settings = false;

    ui.horizontal(|ui|
    {
        if editor_state.selected_type == SelectionType::Object && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Components, " Components");
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Object, "◼ Object");

            object_settings = true;
        }

        if editor_state.selected_type == SelectionType::Camera && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Camera, "📷 Camera");

            camera_settings = true;
        }

        if editor_state.selected_type == SelectionType::Light && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Light, "💡 Light");

            light_settings = true;
        }

        if editor_state.selected_type == SelectionType::Material && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Material, "🎨 Material");

            material_settings = true;
        }

        if editor_state.selected_type == SelectionType::Sound && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Sound, "🔊 Sound");

            sound_settings = true;
        }

        if editor_state.selected_scene_id.is_some()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Scene, "🎬 Scene");
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Debug, "🐛 Debug");
        }

        if editor_state.selected_type == SelectionType::Texture && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Texture, "🖼 Texture");

            texture_settings = true;
        }

        if editor_state.selected_type == SelectionType::SoundSource && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::SoundSource, "🔊 Sound Source");

            sound_source_settings = true;
        }

        if editor_state.selected_type == SelectionType::MeshResource && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::MeshResource, "🔷 Mesh Resource");

            mesh_resource_settings = true;
        }

        ui.selectable_value(&mut editor_state.settings, SettingsPanel::General, "⛭ General");
    });
    ui.separator();

    ScrollArea::vertical().show(ui, |ui|
    {
        match editor_state.settings
        {
            SettingsPanel::Components => if object_settings
            {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
                {
                    create_component_settings(editor_state, state, ui);
                });
            },
            SettingsPanel::Object => if object_settings { create_object_settings(editor_state, state, ui); },
            SettingsPanel::Material => if material_settings { create_material_settings(editor_state, state, ui); },
            SettingsPanel::Camera => if camera_settings { create_camera_settings(editor_state, state, ui); },
            SettingsPanel::Texture => if texture_settings { create_texture_settings(editor_state, state, ui);},
            SettingsPanel::SoundSource => if sound_source_settings { create_sound_source_settings(editor_state, state, ui);},
            SettingsPanel::MeshResource => if mesh_resource_settings { create_mesh_resource_settings(editor_state, state, ui);},
            SettingsPanel::Sound => if sound_settings { create_sound_settings(editor_state, state, ui);},
            SettingsPanel::Light => if light_settings { create_light_settings(editor_state, state, ui); },
            SettingsPanel::Scene => create_scene_settings(editor_state, state, ui),
            SettingsPanel::General => create_general_settings(editor_state, state, ui),
            SettingsPanel::Debug => create_debug_settings(editor_state, state, ui),
            SettingsPanel::Resources => create_general_settings(editor_state, state, ui),
        }
    });
}


fn create_hierarchy(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.horizontal(|ui|
    {
        ui.label("🔍");
        ui.add(egui::TextEdit::singleline(&mut editor_state.hierarchy_filter).desired_width(120.0));

        ui.toggle_value(&mut editor_state.hierarchy_expand_all, "⊞").on_hover_text("expand all items");
    });

    ui.horizontal(|ui|
    {
        ui.checkbox(&mut editor_state.show_internal_entries, "Show Internal Entries").on_hover_text("Show nodes that are used by the editor, like the grid or the camera node.");
    });

    ui.separator();

    // ******************* scenes *******************
    let exec_queue = state.main_thread_execution_queue.clone();

    let mut scenes = vec![];
    swap(&mut state.scenes, &mut scenes);
    for scene in &mut scenes
    {
        let scene_id = scene.id;
        let id = format!("scene_{}", scene_id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, true).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() && editor_state.selected_type == SelectionType::None { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("🎬 {}: {}", scene_id, scene.name)).strong());

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::None;
                        editor_state.settings = SettingsPanel::Scene;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.settings = SettingsPanel::General;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Clear").clicked()
                    {
                        ui.close();
                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            scene.clear(false, false);
                        }));
                    }

                    if ui.button("Clear with Resources").clicked()
                    {
                        ui.close();

                        execute_on_scene_mut(exec_queue.clone(), scene_id, Box::new(move |scene|
                        {
                            scene.clear(false, true);
                        }));
                    }
                });
            });
        }).body(|ui|
        {
            //self.build_node_list(ui, &scene.nodes, scene_id, true);
            create_hierarchy_type_entries(state, editor_state, exec_queue.clone(), scene, ui);
        });
    }

    swap(&mut scenes, &mut state.scenes);

    ui.separator();

    // ******************* resources *******************

    let exec_queue = state.main_thread_execution_queue.clone();

    let ui_id = ui.make_persistent_id("resources");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, true).show_header(ui, |ui|
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            let mut selection; if editor_state.selected_scene_id == None && editor_state.settings == SettingsPanel::Resources && editor_state.selected_object.is_empty() && editor_state.selected_type == SelectionType::None { selection = true; } else { selection = false; }
            let toggle = ui.toggle_value(&mut selection, RichText::new("🗄 Resources").strong());

            if toggle.clicked()
            {
                if selection
                {
                    editor_state.selected_scene_id = None;
                    editor_state.selected_object.clear();
                    editor_state.selected_type = SelectionType::None;
                    editor_state.settings = SettingsPanel::Resources;
                }
                else
                {
                    editor_state.selected_scene_id = None;
                    editor_state.settings = SettingsPanel::General;
                }
            }
        });
    }).body(|ui|
    {
        //self.build_node_list(ui, &scene.nodes, scene_id, true);
        create_resources_entries(state, editor_state, exec_queue.clone(), ui);
    });

}

fn create_hierarchy_type_entries(_state: &mut State, editor_state: &mut EditorState, exec_queue: ExecutionQueueItem, scene: &mut Box<Scene>, ui: &mut Ui)
{
    let scene_id = scene.id;

    let show_internal = editor_state.show_internal_entries;

    // objects
    {
        let id = format!("objects_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Object { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("◼ Objects ({})", scene.get_node_amount_recursive(show_internal))).color(Color32::LIGHT_GREEN).strong()).on_hover_text("there are maybe some internal objects hidden");

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Object;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("⊞ Add New Node").clicked()
                    {
                        ui.close();
                        scene.add_empty_node("Node", None);
                    }
                });
            });
        }).body(|ui|
        {
            let nodes = scene.nodes.clone();
            build_objects_list(editor_state, exec_queue.clone(), scene, ui, &nodes, scene.id, true, false);
        });
    }

    // cameras
    {
        let id = format!("cameras_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut cameras_amount = scene.cameras.len();
                if !show_internal
                {
                    cameras_amount = scene.cameras.iter().filter(|cam| !cam.tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Camera { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("📷 Cameras ({})", cameras_amount)).color(Color32::LIGHT_RED).strong());

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Camera;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Add New Camera").clicked()
                    {
                        ui.close();
                        scene.add_empty_camera("Camera");
                    }
                });
            });
        }).body(|ui|
        {
            build_camera_list(editor_state, &scene.cameras, ui, scene_id);
        });
    }

    // lights
    {
        let id = format!("lights_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut lights_amount = scene.lights.get_ref().len();
                if !show_internal
                {
                    lights_amount = scene.lights.get_ref().iter().filter(|light| !light.borrow().get_ref().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Light { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("💡 Lights ({})", lights_amount)).color(Color32::YELLOW).strong());

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Light;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Add New Light").clicked()
                    {
                        ui.close();
                        scene.add_empty_light("Light");
                    }
                });
            });
        }).body(|ui|
        {
            build_light_list(editor_state, &scene.lights, ui, scene_id);
        });
    }

    // materials
    {
        let id = format!("materials_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut materials_amount = scene.materials.len();
                if !show_internal
                {
                    materials_amount = scene.materials.iter().filter(|(_, material)| !material.read().unwrap().get_base().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Material { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("🎨 Materials ({})", materials_amount)).color(Color32::GOLD).strong());

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Material;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Add New Material").clicked()
                    {
                        scene.add_empty_material("Material");
                        ui.close();
                    }
                });
            });
        }).body(|ui|
        {
            build_material_list(editor_state, &scene.materials, ui, scene_id);
        });
    }
}

fn create_resources_entries(state: &mut State, editor_state: &mut EditorState, exec_queue: ExecutionQueueItem, ui: &mut Ui)
{
    let mipmapping = state.rendering.create_mipmaps;
    let max_tex_res = state.max_texture_resolution();

    let show_internal = editor_state.show_internal_entries;

    // textures
    {
        let ui_id = ui.make_persistent_id("textures");
        let exec_queue = exec_queue.clone();
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut textures_amount = state.resources.textures.len();
                if !show_internal
                {
                    textures_amount = state.resources.textures.iter().filter(|(_, texture)| !texture.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == None && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Texture { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("🖼 Textures ({})", textures_amount)).color(Color32::LIGHT_BLUE).strong());

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Texture;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Add New Texture").clicked()
                    {
                        let exec_queue = exec_queue.clone();
                        spawn_thread(move ||
                        {
                            load_texture_dialog(exec_queue.clone(), None, None, None, mipmapping, max_tex_res);
                        });
                        ui.close();
                    }
                });
            });
        }).body(|ui|
        {
            build_texture_list(editor_state, &state, ui);
        });
    }

    // meshe resources
    {
        let ui_id = ui.make_persistent_id("mesh_resource");
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut mesh_resources_amount = state.resources.mesh_resources.len();
                if !show_internal
                {
                    mesh_resources_amount = state.resources.mesh_resources.iter().filter(|(_, mesh_resource)| !mesh_resource.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == None && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::MeshResource { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new(format!("🔷 Mesh Resources ({})", mesh_resources_amount)).color(Color32::LIGHT_YELLOW).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::MeshResource;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }
            });
        }).body(|ui|
        {
            build_mesh_resources_list(editor_state, &state.resources.mesh_resources, ui);
        });
    }

    // sound sources
    {
        let ui_id = ui.make_persistent_id("sound_sources");
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut sound_sources_amount = state.resources.sound_sources.len();
                if !show_internal
                {
                    sound_sources_amount = state.resources.sound_sources.iter().filter(|(_, sound_source)| !sound_source.read().unwrap().tags.contains_starts_with(ENGINE_INTERNAL_TAG_PREFX)).count();
                }

                let mut selection; if editor_state.selected_scene_id == None && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::SoundSource { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new(format!("🔊 Sound Sources ({})", sound_sources_amount)).color(Color32::LIGHT_GRAY).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::SoundSource;
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.selected_type = SelectionType::None;
                    }
                }
            });
        }).body(|ui|
        {
            build_sound_sources_list(editor_state, &state.resources.sound_sources, ui);
        });
    }
}