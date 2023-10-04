use crate::state::gui::helper::generic_items::{collapse_with_title, self};
use crate::state::{gui::editor::editor_state::EditorState, state::State};
use crate::state::gui::editor::editor_state::SettingsPanel;
use crate::state::scene::scene::Scene;
use egui::{Visuals, Style, ScrollArea, Ui, RichText, Color32};

use super::cameras::{build_camera_list, create_camera_settings};
use super::editor_state::{SelectionType, BottomPanel};
use super::lights::{build_light_list, create_light_settings};
use super::materials::{build_material_list, create_material_settings};
use super::modals::create_component_add_modal;
use super::objects::{build_objects_list, create_object_settings, create_component_settings};
use super::rendering::create_rendering_settings;
use super::scenes::create_scene_settings;
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

    let frame = egui::Frame::side_top_panel(&style);

    egui::TopBottomPanel::top("top_panel").frame(frame).show(ctx, |ui|
    //egui::TopBottomPanel::top("top_panel").show(ctx, |ui|
    {
        ui.horizontal(|ui|
        {
            create_file_menu(state, ui);
        });
    });

    //bottom
    egui::TopBottomPanel::bottom("bottom_panel").frame(frame).show(ctx, |ui|
    {
        ui.horizontal(|ui|
        {
            ui.selectable_value(&mut editor_state.bottom, BottomPanel::Assets, "📦 Assets");
            ui.selectable_value(&mut editor_state.bottom, BottomPanel::Debug, "🐛 Debug");
            ui.selectable_value(&mut editor_state.bottom, BottomPanel::Console, "📝 Console");
        });
        ui.separator();
    });

    //left
    egui::SidePanel::left("left_panel").frame(frame).show(ctx, |ui|
    {
        ui.set_min_width(300.0);

        create_left_sidebar(editor_state, state, ui);
    });

    //right
    egui::SidePanel::right("right_panel").frame(frame).show(ctx, |ui|
    {
        ui.set_min_width(300.0);

        create_right_sidebar(editor_state, state, ui);
    });

    //top
    egui::TopBottomPanel::top("top_panel_main").frame(frame).show(ctx, |ui|
    {
        ui.horizontal(|ui|
        {
            create_tool_menu(editor_state, state, ui);
        });
    });

    // create component
    create_component_add_modal(editor_state, state, ctx);

}

fn create_file_menu(state: &mut State, ui: &mut Ui)
{
    ui.menu_button("File", |ui|
    {
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
        let mut try_out = editor_state.try_out;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui|
        {
            // selectable
            if ui.toggle_value(&mut editor_state.selectable, RichText::new("🖱").size(icon_size)).on_hover_text("select objects").changed()
            {
                if !editor_state.selectable
                {
                    editor_state.de_select_current_item(state);
                }
            }

            ui.toggle_value(&mut editor_state.fly_camera, RichText::new("✈").size(icon_size)).on_hover_text("fly camera");
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
        {
            // fullscreen change
            if ui.toggle_value(&mut fullscreen, RichText::new("⛶").size(icon_size)).on_hover_text("fullscreen").changed()
            {
                state.rendering.fullscreen.set(fullscreen);
            }

            // try out mode
            if ui.toggle_value(&mut try_out, RichText::new("🚀").size(icon_size)).on_hover_text("try out").changed()
            {
                editor_state.set_try_out(state, try_out);
            };
        });
    });
}

fn create_left_sidebar(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // statistics
    collapse_with_title(ui, "chart", true, "📈 Chart", |ui|
    {
        create_chart(editor_state, state, ui);
    });

    // statistics
    collapse_with_title(ui, "statistic", true, "ℹ Statistics", |ui|
    {
        create_statistic(editor_state, state, ui);
    });

    // hierarchy
    collapse_with_title(ui, "hierarchy", true, "🗄 Hierarchy", |ui|
    {
        ScrollArea::vertical().show(ui, |ui|
        {
            create_hierarchy(editor_state, state, ui);
        });
    });
}

fn create_right_sidebar(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let mut object_settings = false;
    let mut camera_settings = false;
    let mut light_settings = false;
    let mut material_settings = false;
    let mut texture_settings = false;

    ui.horizontal(|ui|
    {
        if editor_state.selected_type == SelectionType::Objects && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Components, " Components");
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Object, "◼ Object");

            object_settings = true;
        }

        if editor_state.selected_type == SelectionType::Cameras && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Camera, "📷 Camera");

            camera_settings = true;
        }

        if editor_state.selected_type == SelectionType::Lights && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Light, "💡 Light");

            light_settings = true;
        }

        if editor_state.selected_type == SelectionType::Materials && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Material, "🎨 Material");

            material_settings = true;
        }

        if editor_state.selected_type == SelectionType::Textures && !editor_state.selected_object.is_empty()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Texture, "🖼 Texture");

            texture_settings = true;
        }

        if editor_state.selected_scene_id.is_some()
        {
            ui.selectable_value(&mut editor_state.settings, SettingsPanel::Scene, "🎬 Scene");
        }

        ui.selectable_value(&mut editor_state.settings, SettingsPanel::Rendering, "📷 Rendering");
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
            SettingsPanel::Light => if light_settings { create_light_settings(editor_state, state, ui); },
            SettingsPanel::Scene => create_scene_settings(editor_state, state, ui),
            SettingsPanel::Rendering => create_rendering_settings(editor_state, state, ui),
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

    for scene in &mut state.scenes
    {
        let scene_id = scene.id;
        let id = format!("scene_{}", scene_id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() && editor_state.selected_type == SelectionType::None { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new(format!("🎬 {}: {}", scene_id, scene.name)).strong()).clicked()
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
                        editor_state.settings = SettingsPanel::Rendering;
                    }
                }
            });
        }).body(|ui|
        {
            //self.build_node_list(ui, &scene.nodes, scene_id, true);
            create_hierarchy_type_entries(editor_state, scene, ui);
        });
    }
}

fn create_hierarchy_type_entries(editor_state: &mut EditorState, scene: &mut Box<Scene>, ui: &mut Ui)
{
    let scene_id = scene.id;

    // objects
    {
        let id = format!("objects_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Objects { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new("◼ Objects").color(Color32::LIGHT_GREEN).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Objects;
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
            build_objects_list(editor_state, ui, &scene.nodes, scene.id, true);
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
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Cameras { selection = true; } else { selection = false; }

                let toggle = ui.toggle_value(&mut selection, RichText::new("📷 Cameras").color(Color32::LIGHT_RED).strong());
                let toggle = toggle.context_menu(|ui|
                {
                    if ui.button("Add New Camera").clicked()
                    {
                        ui.close_menu();
                        scene.add_camera("Camera");
                    }
                });

                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Cameras;
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
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Lights { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new("💡 Lights").color(Color32::YELLOW).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Lights;
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
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Materials { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new("🎨 Materials").color(Color32::GOLD).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Materials;
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
            build_material_list(editor_state, &scene.materials, ui, scene_id);
        });
    }

    // textures
    {
        let id = format!("textures_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, editor_state.hierarchy_expand_all).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Textures { selection = true; } else { selection = false; }
                if ui.toggle_value(&mut selection, RichText::new("🖼 Textures").color(Color32::LIGHT_BLUE).strong()).clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::Textures;
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
            build_texture_list(editor_state, &scene.textures, ui, scene_id);
        });
    }
}