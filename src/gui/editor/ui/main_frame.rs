
use std::mem::swap;

use crate::gui::editor::editor::EDITOR_INTERNAL_TAG;
use crate::gui::editor::grid::GRID_ROOT_NAME_XZ_MAIN;
use crate::gui::editor::ui::scene_tabs::create_scene_tabs;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::console_log;
use crate::gui::editor::helper::get_object_and_pointer_world_position;
use crate::gui::editor::ui::console::create_console_section;
use crate::gui::editor::ui::debug::create_debug_settings;
use crate::gui::editor::ui::dialogs::load_texture_dialog;
use crate::gui::editor::ui::helper::ui_helper::loading_progress_bar;
use crate::gui::editor::ui::mesh::{build_mesh_resources_list, create_mesh_resource_settings};
use crate::state::scene::utilities::scene_utils::{execute_on_scene_mut, execute_on_state_mut, move_nodes_to};
use crate::state::state::{ENGINE_INTERNAL_TAG, ENGINE_INTERNAL_TAG_PREFX};
use crate::{component_downcast, component_downcast_mut};
use crate::helper::concurrency::execution_queue::ExecutionQueueItem;
use crate::gui::helper::generic_items::{collapse_with_title, tab, tab_separator};
use crate::state::scene::components::component::ComponentItem;
use crate::state::scene::components::transformation::Transformation;
use crate::state::state::State;
use crate::gui::editor::editor_state::{EditorState, SettingsPanel};
use crate::state::scene::scene::Scene;
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
use super::project::create_project_settings;
use super::scenes::create_scene_settings;
use super::sound::{build_sound_sources_list, create_sound_settings, create_sound_source_settings};
use super::statistics::{create_chart, create_statistic};
use super::textures::{create_texture_settings, build_texture_list};

pub fn create_frame(ui: &mut egui::Ui, editor_state: &mut EditorState, state: &mut State)
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
    let loading_progress: f32 = *editor_state.loading_progress.read().unwrap();

    let frame = egui::Frame::side_top_panel(&style);

    egui::Panel::top("top_panel").frame(frame).show(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            create_file_menu(editor_state, state, ui);
        });
    });

    // status bar — just a single row at the bottom of the screen, separate from the bottom panel
    egui::Panel::bottom("bottom_status_panel").resizable(true).frame(frame).show(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
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

                let multi_select_amount = editor_state.hierarchy_multi_select.len();

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
                else if multi_select_amount > 1
                {
                    ui.separator();
                    ui.label(RichText::new(format!("Selected: {} objects", multi_select_amount)));
                }
            });
        });
    });

    // bottom panel
    if editor_state.bottom_panel_open
    {
        egui::Panel::bottom("bottom_panel").resizable(true).frame(frame).show(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.spacing_mut().item_spacing.x = 2.0;

                if tab(ui, "📦 Assets", editor_state.bottom == BottomPanel::Assets, false).clicked
                {
                    editor_state.bottom = BottomPanel::Assets;
                }

                let console_log_amount = console_log::get_amount();
                let console_errors = console_log::get_error_amount();
                let console_label = if console_errors > 0
                {
                    egui::RichText::new(format!("📝 Console ({} with Errors)", console_log_amount)).color(egui::Color32::LIGHT_RED)
                }
                else
                {
                    egui::RichText::new(format!("📝 Console ({})", console_log_amount))
                };
                let console_tab = tab(ui, console_label, editor_state.bottom == BottomPanel::Console, false);
                if console_errors > 0
                {
                    console_tab.response.on_hover_text(format!("there are {} errors in the console log", console_errors));
                }
                if console_tab.clicked
                {
                    editor_state.bottom = BottomPanel::Console;
                }

                if tab(ui, "🐛 Debug", editor_state.bottom == BottomPanel::Debug, false).clicked
                {
                    editor_state.bottom = BottomPanel::Debug;
                }
            });
            tab_separator(ui);

            if editor_state.bottom == BottomPanel::Assets
            {
                create_asset_section(editor_state, state, ui);
            }
            else if editor_state.bottom == BottomPanel::Console
            {
                create_console_section(editor_state, state, ui);
            }
            else if editor_state.bottom == BottomPanel::Debug
            {
                create_debug_settings(editor_state, state, ui);
            }
        });
    }

    // left panel
    if editor_state.left_panel_open
    {
        egui::Panel::left("left_panel").frame(frame).min_size(300.0).show(ui, |ui|
        {
            ui.set_max_width(ui.available_width());

            //ui.add_enabled_ui(!loading, |ui|
            //{
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui|
                {
                    create_left_sidebar(editor_state, state, ui);
                });
            //});
        });
    }

    // right panel
    if editor_state.right_panel_open
    {
        egui::Panel::right("right_panel").frame(frame).show(ui, |ui|
        {
            ui.set_min_width(300.0);

            //ui.add_enabled_ui(!loading, |ui|
            //{
                create_right_sidebar(editor_state, state, ui);
            //});
        });
    }

    // scene tabs — no bottom inner margin so the tabs sit flush on the panel separator
    let scene_tabs_frame = frame.inner_margin(egui::Margin { left: 8, right: 8, top: 2, bottom: 0 });
    egui::Panel::top("scene_tabs_panel").frame(scene_tabs_frame).show(ui, |ui|
    {
        create_scene_tabs(editor_state, state, ui);
    });

    //top
    egui::Panel::top("top_panel_main").frame(frame).show(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            create_tool_menu(editor_state, state, ui);
        });
    });

    // loading progress bar
    if loading
    {
        loading_progress_bar(ui, loading_progress);
    }

    // box select overlay (selection rect / crosshair)
    crate::gui::editor::box_select::draw_box_select_overlay(ui, editor_state, state);

    // modals
    create_modals(editor_state, state, ui.ctx());
}

fn create_file_menu(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;

    ui.menu_button("File", |ui|
    {
        let shortcut_new = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::N);
        if ui.add(egui::Button::new("New").shortcut_text(ui.ctx().format_shortcut(&shortcut_new))).clicked()
        {
            editor_state.show_confirm_dialog
            (
                "New Project",
                "Do you really want to create a new project?\nUnsaved changes will be lost.",
                |editor_state, state|
                {
                    editor_state.reset_project();
                    state.delete_all_scenes(true);
                    state.add_scene("main scene");
                }
            );
        }

        let shortcut_open = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::O);
        if ui.add(egui::Button::new("Load Project...").shortcut_text(ui.ctx().format_shortcut(&shortcut_open))).clicked()
        {
            let loading_state = editor_state.loading.clone();
            let loading_progress_state = editor_state.loading_progress.clone();
            crate::gui::editor::editor_project::load_editor_project_with_dialog(editor_state, state, loading_state, loading_progress_state);
        }

        ui.menu_button("Open Recent", |ui|
        {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.set_min_width(160.0);

            let recents = editor_state.recent_projects.get_latest_items(10);
            if recents.is_empty()
            {
                ui.add_enabled(false, egui::Button::new("(no recent projects)"));
            }
            else
            {
                for path in &recents
                {
                    let stem = crate::helper::file::get_stem(path);
                    let label = if stem.is_empty() { path.clone() } else { stem };
                    if ui.button(label).on_hover_text(path).clicked()
                    {
                        ui.close();
                        let loading_state = editor_state.loading.clone();
                        let loading_progress_state = editor_state.loading_progress.clone();
                        crate::gui::editor::editor_project::load_editor_project_from_path(editor_state, state, path.clone(), loading_state, loading_progress_state);
                    }
                }
            }
        });

        let shortcut_save = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::S);
        if ui.add(egui::Button::new("Save Project").shortcut_text(ui.ctx().format_shortcut(&shortcut_save))).clicked()
        {
            if let Some(path) = crate::gui::editor::editor_project::save_editor_project_with_dialog(editor_state, state, false)
            {
                editor_state.recent_projects.add_and_save(path);
            }
        }

        let shortcut_save_as = egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::S);
        if ui.add(egui::Button::new("Save Project As...").shortcut_text(ui.ctx().format_shortcut(&shortcut_save_as))).clicked()
        {
            if let Some(path) = crate::gui::editor::editor_project::save_editor_project_with_dialog(editor_state, state, true)
            {
                editor_state.recent_projects.add_and_save(path);
            }
        }

        let shortcut_exit = match ui.ctx().os()
        {
            egui::os::OperatingSystem::Mac     => egui::KeyboardShortcut::new(egui::Modifiers::MAC_CMD, egui::Key::Q),
            egui::os::OperatingSystem::Windows => egui::KeyboardShortcut::new(egui::Modifiers::ALT, egui::Key::F4),
            _                                  => egui::KeyboardShortcut::new(egui::Modifiers::CTRL, egui::Key::Q),
        };
        if ui.add(egui::Button::new("Exit").shortcut_text(ui.ctx().format_shortcut(&shortcut_exit))).clicked()
        {
            state.exit = true;
        }
    });

    ui.menu_button("View", |ui|
    {
        if ui.add(egui::Button::new("Fullscreen").shortcut_text("F")).clicked()
        {
            state.rendering.fullscreen.set(!state.rendering.fullscreen.get_ref().clone());
        }

        if ui.add(egui::Button::new("Hide UI").shortcut_text("H")).clicked()
        {
            editor_state.visible = !editor_state.visible;
        }

        let shortcut_quad_view = egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::ALT, egui::Key::Q);
        if ui.add(egui::Button::new("Quad View").shortcut_text(ui.ctx().format_shortcut(&shortcut_quad_view))).clicked()
        {
            editor_state.quad_view = !editor_state.quad_view;
        }
    });

    ui.menu_button("Settings", |ui|
    {
        if ui.add(egui::Button::new("Settings")).clicked()
        {
            editor_state.dialog_settings = true;
        }
    });

    ui.menu_button("Help", |ui|
    {
        if ui.button("Shortcuts").clicked()
        {
            editor_state.dialog_help_shortcuts = true;
        }

        if ui.button("Splash Screen").clicked()
        {
            editor_state.dialog_splash = true;
        }

        if ui.button("About").clicked()
        {
            editor_state.dialog_about = true;
        }
    });

    // sidebar toggles (top-right, VS Code style)
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
    {
        let icon_size = 16.0;

        // right sidebar
        {
            let img = egui::Image::new(egui::include_image!("../../../../resources/icons/sidebar_right.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
            let btn = egui::Button::image(img).selected(editor_state.right_panel_open).frame(true);
            if ui.add(btn).on_hover_text("Toggle right sidebar (Ctrl+Alt+B)").clicked()
            {
                editor_state.right_panel_open = !editor_state.right_panel_open;
            }
        }

        // bottom panel
        {
            let img = egui::Image::new(egui::include_image!("../../../../resources/icons/panel_bottom.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
            let btn = egui::Button::image(img).selected(editor_state.bottom_panel_open).frame(true);
            if ui.add(btn).on_hover_text("Toggle bottom panel (Ctrl+J)").clicked()
            {
                editor_state.bottom_panel_open = !editor_state.bottom_panel_open;
            }
        }

        // left sidebar
        {
            let img = egui::Image::new(egui::include_image!("../../../../resources/icons/sidebar_left.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
            let btn = egui::Button::image(img).selected(editor_state.left_panel_open).frame(true);
            if ui.add(btn).on_hover_text("Toggle left sidebar (Ctrl+B)").clicked()
            {
                editor_state.left_panel_open = !editor_state.left_panel_open;
            }
        }
    });
}

fn create_tool_menu(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let icon_size = 20.0;
    let padding = 4.0;

    ui.horizontal(|ui|
    {
        // fix the row height up front so every widget is centered against the
        // same height (otherwise the grid menu, added first, sticks to the top)
        ui.set_min_height(icon_size + padding * 2.0);

        let mut fullscreen = state.rendering.fullscreen.get_ref().clone();
        let mut try_out = editor_state.try_mode;

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui|
        {
            create_tool_menu_grid(editor_state, state, ui);
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
        {
            ui.spacing_mut().button_padding = egui::vec2(padding, padding);

            // fullscreen change
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/fullscreen.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(fullscreen).frame(true);
                if ui.add(btn).on_hover_text("Fullscreen").clicked()
                {
                    fullscreen = !fullscreen;
                    state.rendering.fullscreen.set(fullscreen);
                }
            }

            // try out mode
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/tryout.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(try_out).frame(true);
                if ui.add(btn).on_hover_text("Try Out").clicked()
                {
                    try_out = !try_out;
                    editor_state.set_try_mode(state, try_out);
                }
            }

            ui.separator();

            // gizmo
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/gizmo_scale.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.gizmo_scale).frame(true);
                if ui.add(btn).on_hover_text("use scale gizmo").clicked()
                {
                    editor_state.gizmo_scale = !editor_state.gizmo_scale;
                    if editor_state.gizmo_scale
                    {
                        editor_state.gizmo_position = false;
                        editor_state.gizmo_rotation = false;
                    }
                }
            }

            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/gizmo_rotation.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.gizmo_rotation).frame(true);
                if ui.add(btn).on_hover_text("use rotation gizmo").clicked()
                {
                    editor_state.gizmo_rotation = !editor_state.gizmo_rotation;
                    if editor_state.gizmo_rotation
                    {
                        editor_state.gizmo_position = false;
                        editor_state.gizmo_scale = false;
                    }
                }
            }

            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/gizmo_position.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.gizmo_position).frame(true);
                if ui.add(btn).on_hover_text("use position gizmo").clicked()
                {
                    editor_state.gizmo_position = !editor_state.gizmo_position;
                    if editor_state.gizmo_position
                    {
                        editor_state.gizmo_rotation = false;
                        editor_state.gizmo_scale = false;
                    }
                }
            }

            ui.separator();

            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/quad_view.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.quad_view).frame(true);
                if ui.add(btn).on_hover_text("use quad view").clicked()
                {
                    editor_state.quad_view = !editor_state.quad_view;
                }
            }

            // wireframe mode
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/wireframe.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let mut btn = egui::Button::image(img).selected(state.rendering.wireframe_mode).frame(true);
                let supported = state.rendering_adapter.wireframe_mode_support;
                if !supported
                {
                    btn = btn.sense(egui::Sense::hover());
                }
                let hover = if supported { "toggle wireframe mode" } else { "wireframe mode not supported by this GPU/backend" };
                if ui.add(btn).on_hover_text(hover).clicked() && supported
                {
                    state.rendering.wireframe_mode = !state.rendering.wireframe_mode;
                }
            }

            // bounding volume rendering (cycles off -> spheres -> boxes)
            {
                let boxes = state.rendering.draw_bounding_boxes;
                let spheres = state.rendering.draw_bounding_spheres;

                // the icon shows the active hull type (box icon while off)
                let img = if spheres && !boxes
                {
                    egui::Image::new(egui::include_image!("../../../../resources/icons/bounding_sphere.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size))
                }
                else
                {
                    egui::Image::new(egui::include_image!("../../../../resources/icons/bounding_box.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size))
                };

                let mut btn = egui::Button::image(img).selected(boxes || spheres).frame(true);
                let supported = state.rendering_adapter.storage_buffer_array_support;
                if !supported
                {
                    btn = btn.sense(egui::Sense::hover());
                }
                let hover = if supported { "toggle bounding volume rendering (off -> spheres -> boxes)" } else { "bounding volume rendering not supported by this GPU/backend" };
                if ui.add(btn).on_hover_text(hover).clicked() && supported
                {
                    match (boxes, spheres)
                    {
                        (false, false) => { state.rendering.draw_bounding_boxes = true; },
                        (true, false) => { state.rendering.draw_bounding_boxes = false; state.rendering.draw_bounding_spheres = true; },
                        _ => { state.rendering.draw_bounding_boxes = false; state.rendering.draw_bounding_spheres = false; },
                    }
                }
            }

            // x-ray mode (Blender-style see-through)
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/xray.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(state.rendering.xray_mode).frame(true);
                if ui.add(btn).on_hover_text("toggle x-ray mode (see through objects)").clicked()
                {
                    state.rendering.xray_mode = !state.rendering.xray_mode;
                }
            }

            // grid visibility
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/grid.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.grid_visible).frame(true);
                if ui.add(btn).on_hover_text("show/hide grid").clicked()
                {
                    editor_state.grid_visible = !editor_state.grid_visible;
                }
            }

            ui.separator();

            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/highlight.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.use_highlight).frame(true);
                if ui.add(btn).on_hover_text("highlight objects in the editor").clicked()
                {
                    editor_state.use_highlight = !editor_state.use_highlight;
                    if !editor_state.use_highlight
                    {
                        editor_state.remove_highlight(state);
                    }
                    else
                    {
                        editor_state.apply_highlight(state);
                    }
                }
            }

            ui.separator();

            // fly camera
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/fly_camera.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.fly_camera).frame(true);
                if ui.add(btn).on_hover_text("fly camera").clicked()
                {
                    editor_state.fly_camera = !editor_state.fly_camera;
                }
            }

            // frame scene (fit the editor camera to the whole scene)
            {
                use crate::gui::editor::editor::{EDITOR_INTERNAL_TAG, QUAD_CAM};
                use crate::state::scene::node::NodeItem;
                use std::sync::Arc;

                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/frame_scene.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).frame(true);
                if ui.add(btn).on_hover_text("focus scene (fit camera to the whole scene)").clicked()
                {
                    if let Some(scene_id) = state.get_active_scene_id()
                    {
                        if let Some(scene) = state.find_scene_by_id_mut(scene_id)
                        {
                            // the main editor camera (perspective, single view)
                            let cam_index = scene.cameras.iter().position(|cam| cam.tags.contains(EDITOR_INTERNAL_TAG) && !cam.tags.contains(QUAD_CAM));

                            if let Some(cam_index) = cam_index
                            {
                                // exclude editor helpers (grid, gizmo, ...) from the bounding box
                                let predicate: Option<Arc<dyn Fn(NodeItem) -> bool + Send + Sync>> = Some(Arc::new(|node: NodeItem|
                                {
                                    !node.read().unwrap().tags.contains(EDITOR_INTERNAL_TAG)
                                }));

                                crate::state::scene::utilities::scene_utils::align_camera_to_scene(scene, cam_index, None, None, predicate);
                            }
                        }
                    }
                }
            }

            // selectable
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/select.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(editor_state.selectable).frame(true);
                if ui.add(btn).on_hover_text("select objects").clicked()
                {
                    editor_state.selectable = !editor_state.selectable;
                    if !editor_state.selectable
                    {
                        editor_state.de_select_current_item(state);
                    }
                }
            }

            ui.separator();

            // play/pause
            {
                let playing = !state.pause;
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/engine.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size));
                let btn = egui::Button::image(img).selected(playing).frame(true);
                if ui.add(btn).on_hover_text("Playing/Pause").clicked()
                {
                    state.pause = playing;
                }
            }
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
            let grid = scene.find_node_by_name(GRID_ROOT_NAME_XZ_MAIN);

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

                let row_width = ui.available_width();
                let row_height = ui.spacing().interact_size.y;
                ui.allocate_ui(egui::vec2(row_width, row_height), |ui|
                {
                    ui.horizontal(|ui|
                    {
                        let icon_size = 14.0;

                        // add button
                        let add_btn = ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../../../../resources/icons/add.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size))));
                        egui::Popup::menu(&add_btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui|
                        {
                            ui.set_min_width(120.0);
                            if ui.button("⊞ Add Scene").clicked()
                            {
                                state.add_scene("Scene");
                                egui::Popup::close_all(ui.ctx());
                            }

                            if ui.button("⬇ Import Scene").clicked()
                            {
                                let loading_state = editor_state.loading.clone();
                                let loading_progress_state = editor_state.loading_progress.clone();
                                if let Some(new_scene_id) = crate::gui::editor::editor_project::import_editor_scene_with_dialog(state, loading_state, loading_progress_state)
                                {
                                    if !editor_state.open_scene_tabs.contains(&new_scene_id)
                                    {
                                        editor_state.open_scene_tabs.push(new_scene_id);
                                    }
                                    editor_state.selected_scene_id = Some(new_scene_id);
                                    editor_state.selected_object.clear();
                                    editor_state.selected_type = SelectionType::None;
                                    editor_state.settings_panel = SettingsPanel::Scene;
                                    state.set_active_scene(new_scene_id);
                                }

                                egui::Popup::close_all(ui.ctx());
                            }
                        });

                        // more button — add right-to-left so TextEdit gets exact remaining space
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                        {
                            let more_btn = ui.add(egui::Button::image(egui::Image::new(egui::include_image!("../../../../resources/icons/more.svg")).fit_to_exact_size(egui::vec2(icon_size, icon_size))));
                            egui::Popup::menu(&more_btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui|
                            {
                                ui.set_min_width(160.0);
                                ui.checkbox(&mut editor_state.show_internal_entries, "Show Internal Entries").on_hover_text("Show nodes that are used by the editor, like the grid or the camera node.");
                            });

                            let search_response = ui.add(egui::TextEdit::singleline(&mut editor_state.hierarchy_filter).desired_width(f32::INFINITY));
                            let icon_rect = egui::Rect::from_center_size(egui::pos2(search_response.rect.right() - icon_size / 2.0 - 4.0, search_response.rect.center().y),egui::vec2(icon_size, icon_size));
                            egui::Image::new(egui::include_image!("../../../../resources/icons/search.svg")).tint(ui.visuals().weak_text_color()).paint_at(ui, icon_rect);
                        });
                    });
                });

                ui.separator();

                // hierarchy
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
        ui.spacing_mut().item_spacing.x = 2.0;

        // selects `panel` when its tab is clicked
        let mut settings_tab = |ui: &mut Ui, panel: SettingsPanel, label: &str|
        {
            if tab(ui, label, editor_state.settings_panel == panel, false).clicked
            {
                editor_state.settings_panel = panel;
            }
        };

        if editor_state.selected_type == SelectionType::Object && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Components, " Components");
            settings_tab(ui, SettingsPanel::Object, "◼ Object");

            object_settings = true;
        }

        if editor_state.selected_type == SelectionType::Camera && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Camera, "📷 Camera");

            camera_settings = true;
        }

        if editor_state.selected_type == SelectionType::Light && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Light, "💡 Light");

            light_settings = true;
        }

        if editor_state.selected_type == SelectionType::Material && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Material, "🎨 Material");

            material_settings = true;
        }

        if editor_state.selected_type == SelectionType::Sound && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Sound, "🔊 Sound");

            sound_settings = true;
        }

        if editor_state.selected_scene_id.is_some()
        {
            settings_tab(ui, SettingsPanel::Scene, "🎬 Scene");
        }

        if editor_state.selected_type == SelectionType::Texture && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::Texture, "🖼 Texture");

            texture_settings = true;
        }

        if editor_state.selected_type == SelectionType::SoundSource && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::SoundSource, "🔊 Sound Source");

            sound_source_settings = true;
        }

        if editor_state.selected_type == SelectionType::MeshResource && !editor_state.selected_object.is_empty()
        {
            settings_tab(ui, SettingsPanel::MeshResource, "🔷 Mesh Resource");

            mesh_resource_settings = true;
        }

        settings_tab(ui, SettingsPanel::General, "⛭ General");
        settings_tab(ui, SettingsPanel::Project, "📋 Project");
    });
    tab_separator(ui);

    ScrollArea::vertical().show(ui, |ui|
    {
        match editor_state.settings_panel
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
            SettingsPanel::Resources => create_general_settings(editor_state, state, ui),
            SettingsPanel::Project => create_project_settings(editor_state, state, ui),
        }
    });
}


fn create_hierarchy(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // ******************* scenes *******************
    let exec_queue = state.main_thread_execution_queue.clone();

    let mut scenes = vec![];
    swap(&mut state.scenes, &mut scenes);
    for scene in &mut scenes
    {
        let is_internal_node = scene.has_tag(ENGINE_INTERNAL_TAG) || scene.has_tag(EDITOR_INTERNAL_TAG);
        let show_from_tags = !is_internal_node || (is_internal_node && editor_state.show_internal_entries);

        if !show_from_tags
        {
            continue;
        }

        let scene_id = scene.id;
        let id = format!("scene_{}", scene_id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, true).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() && editor_state.selected_type == SelectionType::None { selection = true; } else { selection = false; }

                let mut heading = RichText::new(format!("🎬 {}", scene.name)).strong();

                if scene.active
                {
                    heading = heading.color(Color32::LIGHT_BLUE);
                }

                let mut toggle = ui.toggle_value(&mut selection, heading);

                toggle = toggle.on_hover_text(format!("Scene ID: {}", scene.id));
                if toggle.clicked()
                {
                    if selection
                    {
                        editor_state.selected_scene_id = Some(scene_id);
                        editor_state.selected_object.clear();
                        editor_state.selected_type = SelectionType::None;
                        editor_state.settings_panel = SettingsPanel::Scene;
                        if !editor_state.open_scene_tabs.contains(&scene_id)
                        {
                            editor_state.open_scene_tabs.push(scene_id);
                        }
                    }
                    else
                    {
                        editor_state.selected_scene_id = None;
                        editor_state.settings_panel = SettingsPanel::General;
                    }
                }

                toggle.context_menu(|ui|
                {
                    if ui.button("Set Active").clicked()
                    {
                        ui.close();
                        execute_on_state_mut(exec_queue.clone(),  Box::new(move |sate|
                        {
                            sate.set_active_scene(scene_id);
                        }));
                    }

                    ui.separator();

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

                    ui.separator();

                    if ui.button("🗑 Delete").clicked()
                    {
                        ui.close();

                        execute_on_state_mut(exec_queue.clone(), Box::new(move |state|
                        {
                            state.delete_scene_by_id(scene_id, false);
                        }));
                    }

                    if ui.button(RichText::new("🗑 Delete + Clear Resources").color(Color32::LIGHT_RED)).clicked()
                    {
                        ui.close();

                        execute_on_state_mut(exec_queue.clone(), Box::new(move |state|
                        {
                            state.delete_scene_by_id(scene_id, true);
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
            let mut selection; if editor_state.selected_scene_id == None && editor_state.settings_panel == SettingsPanel::Resources && editor_state.selected_object.is_empty() && editor_state.selected_type == SelectionType::None { selection = true; } else { selection = false; }
            let toggle = ui.toggle_value(&mut selection, RichText::new("🗄 Resources").strong());

            if toggle.clicked()
            {
                if selection
                {
                    editor_state.selected_scene_id = None;
                    editor_state.selected_object.clear();
                    editor_state.selected_type = SelectionType::None;
                    editor_state.settings_panel = SettingsPanel::Resources;
                }
                else
                {
                    editor_state.selected_scene_id = None;
                    editor_state.settings_panel = SettingsPanel::General;
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
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
        {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
            {
                let mut selection; if editor_state.selected_scene_id == Some(scene_id) && editor_state.selected_object.is_empty() &&  editor_state.selected_type == SelectionType::Object { selection = true; } else { selection = false; }
                let toggle = ui.toggle_value(&mut selection, RichText::new(format!("◼ Objects ({})", scene.get_node_amount_recursive(show_internal))).color(Color32::LIGHT_GREEN).strong()).on_hover_text("there are maybe some internal objects hidden");

                // *** drop onto root: make nodes top-level (no parent) ***
                let drop_resp = ui.interact(toggle.rect, egui::Id::new(("objects_root_drop", scene_id)), egui::Sense::hover());
                if drop_resp.dnd_hover_payload::<u32>().is_some()
                {
                    ui.painter().rect_stroke(toggle.rect, 2.0, egui::Stroke::new(2.0, egui::Color32::YELLOW), egui::StrokeKind::Outside);
                }
                if let Some(payload) = drop_resp.dnd_release_payload::<u32>()
                {
                    let dragged_id = *payload;
                    let nodes_to_move: Vec<u32> = if editor_state.hierarchy_multi_select.contains(&dragged_id) && !editor_state.hierarchy_multi_select.is_empty()
                    {
                        editor_state.hierarchy_multi_select.clone()
                    }
                    else
                    {
                        vec![dragged_id]
                    };
                    editor_state.hierarchy_multi_select.clear();
                    move_nodes_to(exec_queue.clone(), scene.id, nodes_to_move, None);

                    execute_on_state_mut(exec_queue.clone(), Box::new(move |state|
                    {
                        EditorState::de_select_all_items(state, None);
                    }));
                    editor_state.de_select_current_item_from_scene(scene);
                }

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
                        let node = scene.add_empty_node_front("Node", None);
                        node.write().unwrap().settings.transient = false;
                    }
                });
            });
        }).body(|ui|
        {
            let nodes = scene.nodes.clone();
            let mut flat_order = vec![];
            build_objects_list(editor_state, exec_queue.clone(), scene, ui, &nodes, scene.id, true, false, &mut flat_order);
            editor_state.hierarchy_flat_nodes_order = flat_order;
        });
    }


    // cameras
    {
        let id = format!("cameras_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
                    if ui.button("⊞ Add New Camera").clicked()
                    {
                        ui.close();
                        scene.add_empty_camera("Camera");
                    }
                });
            });
        }).body(|ui|
        {
            build_camera_list(editor_state, exec_queue.clone(), &scene.cameras, ui, scene_id);
        });
    }

    // lights
    {
        let id = format!("lights_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
                    if ui.button("⊞ Add New Light").clicked()
                    {
                        ui.close();
                        scene.add_empty_light("Light");
                    }
                });
            });
        }).body(|ui|
        {
            build_light_list(editor_state, exec_queue.clone(), &scene.lights, ui, scene_id);
        });
    }

    // materials
    {
        let id = format!("materials_{}", scene.id);
        let ui_id = ui.make_persistent_id(id.clone());
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
                    if ui.button("⊞ Add New Material").clicked()
                    {
                        scene.add_empty_material("Material");
                        ui.close();
                    }
                });
            });
        }).body(|ui|
        {
            build_material_list(editor_state, exec_queue.clone(), &scene.materials, ui, scene_id);
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
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, false).show_header(ui, |ui|
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