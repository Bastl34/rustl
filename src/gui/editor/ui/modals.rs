use crate::{gui::{editor::ui::{help::create_modal_help_shortcuts, helper::ui_helper::fit_size}, helper::generic_items::modal_with_title}, helper::generic::cargo_dependencies, state::state::State};

use super::super::editor_state::EditorState;

pub fn create_modals(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    if editor_state.dialog_splash
    {
        create_modal_splash(editor_state, state, ctx);
    }
    else if editor_state.dialog_add_component
    {
        create_modal_component_add(editor_state, state, ctx);
    }
    else if editor_state.dialog_add_camera_controller
    {
        create_modal_camera_controller(editor_state, state, ctx);
    }
    else if editor_state.dialog_add_scene_controller
    {
        create_modal_scene_controller(editor_state, state, ctx);
    }
    else if editor_state.dialog_debug_image
    {
        create_modal_debug_image(editor_state, state, ctx);
    }
    else if editor_state.dialog_settings
    {
        create_modal_settings(editor_state, state, ctx);
    }
    else if editor_state.dialog_help_shortcuts
    {
        create_modal_help_shortcuts(editor_state, ctx);
    }
    else if editor_state.dialog_about
    {
        create_modal_about(editor_state, ctx);
    }
}

pub fn create_modal_component_add(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let mut dialog_add_component = editor_state.dialog_add_component;

    let (_, instance_id) = editor_state.get_object_ids();
    let is_instance = instance_id.is_some();

    modal_with_title(ctx, &mut dialog_add_component, "Add component", false, false, |ui|
    {
        ui.label("Add your component");

        ui.horizontal(|ui|
        {
            ui.label("Name: ");
            ui.text_edit_singleline(&mut editor_state.add_component_name);
        });

        ui.horizontal(|ui|
        {
            ui.label("Component: ");

            let current_component_name = state.registered_components.get(editor_state.add_component_id).unwrap().0.clone();

            egui::ComboBox::from_label("").selected_text(current_component_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(40.0);

                for (component_id, (component_name, component_instantiable, _)) in state.registered_components.iter().enumerate()
                {
                    if !is_instance || (is_instance && *component_instantiable)
                    {
                        ui.selectable_value(&mut editor_state.add_component_id, component_id, component_name.clone());
                    }
                }
            });
        });

        if ui.button("Add").clicked()
        {
            let (node_id, instance_id) = editor_state.get_object_ids();

            if let (Some(scene_id), Some(node_id)) = (editor_state.selected_scene_id, node_id)
            {
                let (_component_name, _component_instantiable, component_func) = state.registered_components.get(editor_state.add_component_id).unwrap().clone();

                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                let node = scene.find_node_by_id(node_id).unwrap();

                if let Some(instance_id) = instance_id
                {
                    let node = node.read().unwrap();
                    let instance = node.find_instance_by_id(instance_id).unwrap();
                    let mut instance = instance.write().unwrap();
                    instance.add_component(component_func(editor_state.add_component_name.as_str()));
                }
                else
                {
                    node.write().unwrap().add_component(component_func(editor_state.add_component_name.as_str()));
                }
            }

            editor_state.dialog_add_component = false;
            editor_state.add_component_name = "Component".to_string();
        }
    });

    if !dialog_add_component
    {
        editor_state.dialog_add_component = dialog_add_component;
    }
}

pub fn create_modal_camera_controller(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let mut dialog_add_camera_controller = editor_state.dialog_add_camera_controller;

    modal_with_title(ctx, &mut dialog_add_camera_controller, "Add Controller", false, false, |ui|
    {
        ui.label("Add Camera Controller");

        ui.horizontal(|ui|
        {
            ui.label("Controller: ");

            let current_component_name = state.registered_camera_controller.get(editor_state.add_camera_controller_id).unwrap().0.clone();

            egui::ComboBox::from_label("").selected_text(current_component_name).width(180.0).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(40.0);

                for (id, controller) in state.registered_camera_controller.iter().enumerate()
                {
                    ui.selectable_value(&mut editor_state.add_camera_controller_id, id, controller.0.clone());
                }
            });
        });

        if ui.button("Add").clicked()
        {
            let (camera_id, ..) = editor_state.get_object_ids();

            if let (Some(scene_id), Some(camera_id)) = (editor_state.selected_scene_id, camera_id)
            {
                let cam_controller = state.registered_camera_controller.get(editor_state.add_camera_controller_id).unwrap().clone();

                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                let camera = scene.get_camera_by_id_mut(camera_id).unwrap();

                camera.controller = Some(cam_controller.1());
            }

            editor_state.dialog_add_camera_controller = false;
        }
    });

    if !dialog_add_camera_controller
    {
        editor_state.dialog_add_camera_controller = dialog_add_camera_controller;
    }
}

pub fn create_modal_scene_controller(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let mut dialog_add_scene_controller = editor_state.dialog_add_scene_controller;
    let post_controller = editor_state.add_scene_controller_post;

    modal_with_title(ctx, &mut dialog_add_scene_controller, "Add Controller", false, false, |ui|
    {
        ui.label("Add Scene Controller");

        ui.horizontal(|ui|
        {
            ui.label("Controller: ");

            let current_component_name = state.registered_scene_controller.get(editor_state.add_scene_controller_id).unwrap().0.clone();

            egui::ComboBox::from_label("").selected_text(current_component_name).width(180.0).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(40.0);

                for (id, controller) in state.registered_scene_controller.iter().enumerate()
                {
                    ui.selectable_value(&mut editor_state.add_scene_controller_id, id, controller.0.clone());
                }
            });
        });

        if ui.button("Add").clicked()
        {
            if let Some(scene_id) = editor_state.selected_scene_id
            {
                let scene_controller = state.registered_scene_controller.get(editor_state.add_scene_controller_id).unwrap().clone();

                let scene = state.find_scene_by_id_mut(scene_id).unwrap();

                if post_controller
                {
                    scene.post_controller.push(scene_controller.1());
                }
                else
                {
                    scene.pre_controller.push(scene_controller.1());
                }
            }

            editor_state.dialog_add_scene_controller = false;
        }
    });

    if !dialog_add_scene_controller
    {
        editor_state.dialog_add_scene_controller = dialog_add_scene_controller;
    }
}

pub fn create_modal_debug_image(editor_state: &mut EditorState, _state: &mut State, ctx: &egui::Context)
{
    let mut dialog_debug_image = editor_state.dialog_debug_image;

    modal_with_title(ctx, &mut dialog_debug_image, "Debug Image", true, true, |ui|
    {
        if let Some(debug_image_id) = editor_state.dialog_debug_image_id.as_ref()
        {
            let avail = ui.available_size();
            let texture_size = debug_image_id.size();
            let tex_size = egui::vec2(texture_size[0] as f32, texture_size[1] as f32);
            let draw_size = fit_size(avail, tex_size);

            ui.allocate_ui(draw_size, |ui|
            {
                ui.image((debug_image_id.id(), draw_size));
            });
        }
        else
        {
            ui.label("No image to display");
        }
    });

    if !dialog_debug_image
    {
        editor_state.dialog_debug_image = dialog_debug_image;
    }
}

pub fn create_modal_settings(editor_state: &mut EditorState, _state: &mut State, ctx: &egui::Context)
{
    let mut dialog_settings = editor_state.dialog_settings;

    modal_with_title(ctx, &mut dialog_settings, "Settings", false, false, |ui|
    {
        ui.set_min_width(360.0);

        editor_state.settings.ui(ui);
    });

    if !dialog_settings
    {
        editor_state.dialog_settings = dialog_settings;
    }
}

pub fn create_modal_about(editor_state: &mut EditorState, ctx: &egui::Context)
{
    let mut dialog_about = editor_state.dialog_about;

    modal_with_title(ctx, &mut dialog_about, "About", true, false, |ui|
    {
        ui.set_min_width(360.0);

        ui.vertical_centered(|ui|
        {
            ui.add_space(8.0);

            let logo_size = 128.0;

            let logo = egui::Image::new(egui::include_image!("../../../../resources/designs/logo/logo.svg")).fit_to_exact_size(egui::vec2(logo_size, logo_size));
            ui.add(logo);

            ui.add_space(8.0);

            ui.label(egui::RichText::new("Rustl").heading().strong());
            ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).weak());

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label("A 3D scene editor and game engine written in Rust,");
            ui.label("powered by wgpu and egui.");

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui|
            {
                ui.label("Author:");
                ui.label(egui::RichText::new("Bastian Karge").strong());
            });

            ui.horizontal(|ui|
            {
                ui.label("Source:");
                ui.hyperlink_to("github.com/Bastl34/rustl", "https://github.com/Bastl34/rustl");
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);

            ui.label(egui::RichText::new("Built with").small().weak());
            ui.add_space(2.0);

            ui.scope(|ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.label(egui::RichText::new(cargo_dependencies().join(", ")).small().weak());
            });

            ui.add_space(8.0);
        });
    });

    if !dialog_about
    {
        editor_state.dialog_about = dialog_about;
    }
}

pub fn create_modal_splash(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let screen_rect = ctx.content_rect();

    // backdrop dimming the editor and catching outside clicks
    egui::Area::new(egui::Id::new("splash_backdrop"))
        .fixed_pos(screen_rect.min)
        .order(egui::Order::Middle)
        .interactable(true)
        .show(ctx, |ui|
        {
            let response = ui.allocate_rect(screen_rect, egui::Sense::click());
            ui.painter().rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(180));

            if response.clicked()
            {
                editor_state.dialog_splash = false;
            }
        });

    // Esc closes splash too
    if ctx.input(|i| i.key_pressed(egui::Key::Escape))
    {
        editor_state.dialog_splash = false;
    }

    let frame = egui::Frame::window(&ctx.global_style())
        .corner_radius(12.0)
        .inner_margin(egui::Margin::same(0))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(20)))
        .shadow(egui::Shadow
        {
            color: egui::Color32::from_black_alpha(180),
            offset: [0, 12],
            blur: 32,
            spread: 0,
        });

    let mut close = false;

    egui::Window::new("splash_window")
        .title_bar(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .frame(frame)
        .order(egui::Order::Foreground)
        .show(ctx, |ui|
        {
            let splash_width  = 520.0;
            let header_height = 220.0;

            ui.set_width(splash_width);

            // header band with patterned background + logo + title (click closes like Continue)
            let (header_rect, header_response) = ui.allocate_exact_size(egui::vec2(splash_width, header_height), egui::Sense::click());
            if header_response.clicked()
            {
                close = true;
            }

            let painter = ui.painter_at(header_rect);

            // solid dark base
            let base_color = egui::Color32::from_rgb(13, 15, 26);
            painter.rect_filled(header_rect, 0.0, base_color);

            // aurora blobs — concentric circles with low alpha fake a soft blur
            let blobs =
            [
                (egui::pos2(header_rect.right() - 70.0, header_rect.top()    + 50.0),  180.0, (255u8,  90, 180)), // pink
                (egui::pos2(header_rect.left()  + 80.0, header_rect.bottom() - 30.0),  200.0, ( 90u8, 140, 255)), // blue
                (egui::pos2(header_rect.center().x - 40.0, header_rect.top() + 20.0),  140.0, (140u8,  80, 255)), // purple
            ];

            for (center, max_radius, (r, g, b)) in blobs
            {
                let layers = 24;
                for i in 0..layers
                {
                    let t = i as f32 / layers as f32;
                    let radius = max_radius * (0.2 + t * 0.8);
                    let alpha = ((1.0 - t).powf(2.0) * 26.0) as u8;
                    if alpha > 0
                    {
                        painter.circle_filled(center, radius, egui::Color32::from_rgba_unmultiplied(r, g, b, alpha));
                    }
                }
            }

            // dotted grid overlay
            let dot_spacing = 14.0;
            let dot_radius  = 1.0;
            let dot_color   = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20);
            let cols = (header_rect.width()  / dot_spacing).ceil() as i32;
            let rows = (header_rect.height() / dot_spacing).ceil() as i32;
            for col in 0..=cols
            {
                for row in 0..=rows
                {
                    let x = header_rect.left() + col as f32 * dot_spacing;
                    let y = header_rect.top()  + row as f32 * dot_spacing;
                    painter.circle_filled(egui::pos2(x, y), dot_radius, dot_color);
                }
            }

            // logo centered in the header
            let logo_size = 130.0;
            let logo_rect = egui::Rect::from_center_size(header_rect.center(), egui::vec2(logo_size, logo_size));
            egui::Image::new(egui::include_image!("../../../../resources/designs/logo/logo.svg")).paint_at(ui, logo_rect);

            // top-left: title + subtitle
            let inset = 12.0;
            painter.text
            (
                egui::pos2(header_rect.left() + inset, header_rect.top() + inset),
                egui::Align2::LEFT_TOP,
                "Rustl",
                egui::FontId::proportional(28.0),
                egui::Color32::WHITE,
            );

            // top-right: version
            painter.text
            (
                egui::pos2(header_rect.right() - inset, header_rect.top() + inset),
                egui::Align2::RIGHT_TOP,
                format!("{}", env!("CARGO_PKG_VERSION")),
                egui::FontId::proportional(13.0),
                egui::Color32::from_white_alpha(180),
            );

            // bottom-left: clickable github repo link (read from Cargo.toml)
            let repo_url     = env!("CARGO_PKG_REPOSITORY");
            let repo_display = repo_url.trim_start_matches("https://").trim_start_matches("http://");
            let link_size    = egui::vec2(260.0, 18.0);
            let link_rect    = egui::Rect::from_min_size(
                egui::pos2(header_rect.left() + inset, header_rect.bottom() - inset - link_size.y),
                link_size,
            );
            ui.scope_builder
            (
                egui::UiBuilder::new()
                    .max_rect(link_rect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui|
                {
                    ui.hyperlink_to
                    (
                        egui::RichText::new(repo_display).size(12.0).color(egui::Color32::from_white_alpha(180)),
                        repo_url,
                    );
                },
            );

            // body
            ui.scope(|ui|
            {
                ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 6.0);

                ui.separator();

                let recents = editor_state.recent_projects.get_latest_items(10);
                let inset = 20.0;
                let row_height = 38.0;
                let row_width = splash_width - inset * 2.0;

                if !recents.is_empty()
                {
                    ui.add_space(12.0);

                    ui.horizontal(|ui|
                    {
                        ui.add_space(inset);
                        ui.label(egui::RichText::new("Recent Projects").size(12.0).color(egui::Color32::from_white_alpha(150)).strong());
                    });
                    ui.add_space(4.0);

                    let mut chosen: Option<String> = None;

                    egui::ScrollArea::vertical().max_height(130.0).id_salt("splash_recent_scroll").show(ui, |ui|
                    {
                        for path in &recents
                        {
                            let stem = crate::helper::file::get_stem(path);
                            let display_name = if stem.is_empty() { path.clone() } else { stem };

                            ui.horizontal(|ui|
                            {
                                ui.add_space(inset);

                                let (rect, response) = ui.allocate_exact_size
                                (
                                    egui::vec2(row_width, row_height),
                                    egui::Sense::click(),
                                );

                                let bg = if response.hovered()
                                {
                                    egui::Color32::from_white_alpha(22)
                                }
                                else
                                {
                                    egui::Color32::from_white_alpha(6)
                                };
                                ui.painter().rect_filled(rect, 6.0, bg);

                                let row_painter = ui.painter_at(rect);
                                let text_inset = 12.0;
                                row_painter.text
                                (
                                    egui::pos2(rect.left() + text_inset, rect.top() + 7.0),
                                    egui::Align2::LEFT_TOP,
                                    &display_name,
                                    egui::FontId::proportional(13.0),
                                    egui::Color32::WHITE,
                                );
                                row_painter.text
                                (
                                    egui::pos2(rect.left() + text_inset, rect.bottom() - 7.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    path,
                                    egui::FontId::proportional(10.0),
                                    egui::Color32::from_white_alpha(140),
                                );

                                let response = response.on_hover_text(path);
                                if response.clicked()
                                {
                                    chosen = Some(path.clone());
                                }
                            });
                        }
                    });

                    if let Some(path) = chosen
                    {
                        let loading_state = editor_state.loading.clone();
                        let loading_progress_state = editor_state.loading_progress.clone();
                        crate::gui::editor::editor_project::load_editor_project_from_path(editor_state, state, path, loading_state, loading_progress_state);
                        close = true;
                    }

                    ui.add_space(10.0);
                    ui.separator();
                }

                ui.add_space(10.0);

                ui.horizontal(|ui|
                {
                    ui.add_space(inset);

                    if ui.button("Open Project...").clicked()
                    {
                        let loading_state = editor_state.loading.clone();
                        let loading_progress_state = editor_state.loading_progress.clone();
                        crate::gui::editor::editor_project::load_editor_project_with_dialog(editor_state, state, loading_state, loading_progress_state);
                        close = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                    {
                        ui.add_space(inset);
                        if ui.button(egui::RichText::new("Continue").strong()).clicked()
                        {
                            close = true;
                        }
                    });
                });

                ui.add_space(14.0);
            });
        });

    if close
    {
        editor_state.dialog_splash = false;
    }
}
