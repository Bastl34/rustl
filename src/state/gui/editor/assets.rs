use egui::{Ui, ScrollArea, Id, Color32, RichText};
use wgpu::Color;

use crate::state::{gui::helper::generic_items::separator_colored, state::State};

use super::editor_state::{EditorState, AssetType};

pub fn create_asset_section(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.set_min_height(220.0);

    ui.horizontal_top(|ui|
    {
        create_asset_tree(editor_state, state, ui);
        create_asset_list(editor_state, state, ui);
    });
}

pub fn create_asset_tree(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.scope(|ui|
    {
        ui.set_min_width(100.0);
        //ui.set_max_width(100.0);

        ui.vertical(|ui|
        {
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Scene, "🎬 Scene");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Object, "📦 Object");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Texture, "🖼 Texture");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Material, "🎨 Material");
        });
    });
}

pub fn create_asset_list(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let items = match editor_state.asset_type
    {
        AssetType::Scene => Some(&editor_state.scenes),
        AssetType::Object => Some(&editor_state.objects),
        _ => None
    };

    if items.is_none() { return; }
    let items = items.unwrap();

    ui.vertical(|ui|
    {
        ui.horizontal(|ui|
        {
            if editor_state.asset_type == AssetType::Object || editor_state.asset_type == AssetType::Scene
            {
                ui.label("🔍");
                ui.add(egui::TextEdit::singleline(&mut editor_state.asset_filter).desired_width(100.0));
            }

            if editor_state.asset_type == AssetType::Object
            {
                ui.checkbox(&mut editor_state.reuse_materials_by_name, "Reuse Materials by name");
            }
        });

        ScrollArea::vertical().show(ui, |ui|
        {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());

            ui.horizontal_wrapped(|ui|
            {
                for asset in items
                {
                    let filter = editor_state.asset_filter.to_lowercase();

                    if !filter.is_empty() && asset.name.to_lowercase().find(filter.as_str()).is_none()
                    {
                        continue;
                    }

                    let str_id = format!("{} asset", asset.path);
                    let item_id = Id::new(str_id.clone());
                    let name = asset.name.clone();
                    let str_id_inner = format!("{}_inner", str_id.clone());

                    let width = 100.0;
                    let height = 150.0;
                    let margin = 2.0;
                    let image_size = width - 20.0;

                    let bg_color = Color32::from_white_alpha(3);
                    let highlight_color = egui::Color32::from_rgba_premultiplied(0, 100, 210, 50);
                    let separator_color = Color32::LIGHT_GRAY;
                    let image_background_color = Color32::from_rgba_premultiplied(0, 0, 0, 150);

                    let shadow = egui::Shadow
                    {
                        offset: [2.0, 2.0].into(),
                        blur: 4.0,
                        spread: 2.0,
                        color: egui::Color32::from_black_alpha(180),
                        //color: egui::Color32::from_white_alpha(180)
                    };

                    let apply_size = |ui: &mut Ui|
                    {
                        ui.set_min_width(width);
                        ui.set_max_width(width);
                        ui.set_min_height(height);
                        ui.set_max_height(height);
                    };

                    let apply_available_size = |ui: &mut Ui|
                    {
                        ui.set_min_width(ui.available_width());
                        ui.set_max_width(ui.available_width());
                        ui.set_min_height(ui.available_height());
                        ui.set_max_height(ui.available_height());
                    };

                    // if is_being_dragged
                    let is_being_dragged = ui.ctx().is_being_dragged(item_id);
                    if is_being_dragged
                    {
                        editor_state.drag_id = Some(asset.path.clone());
                    }

                    ui.allocate_ui(egui::Vec2::new(width + (margin * 2.0), height + (margin * 2.0)), |ui|
                    {
                        apply_available_size(ui);

                        ui.dnd_drag_source(item_id, asset.path.clone(), |ui|
                        {
                            apply_available_size(ui);

                            ui.push_id(str_id_inner, |ui|
                            {
                                apply_available_size(ui);

                                let mut frame = egui::Frame::default().fill(bg_color).rounding(2.0).shadow(shadow).outer_margin(margin);
                                if is_being_dragged
                                {
                                    frame = frame.fill(highlight_color).stroke(egui::Stroke::new(2.0, highlight_color));
                                }

                                frame.show(ui, |ui|
                                {
                                    apply_size(ui);

                                    ui.vertical(|ui|
                                    {
                                        ui.vertical_centered(|ui|
                                        {
                                            egui::Frame::default().fill(image_background_color).show(ui, |ui|
                                            {
                                                ui.set_min_width(ui.available_width());
                                                ui.set_max_width(ui.available_width());

                                                ui.allocate_ui(egui::Vec2::new(image_size, image_size), |ui|
                                                {
                                                    apply_available_size(ui);

                                                    if let Some(egui_preview) = &asset.egui_preview
                                                    {
                                                        ui.image((egui_preview.id(), egui::Vec2::new(ui.available_width(), ui.available_height())));
                                                    }
                                                    else
                                                    {
                                                        ui.label(RichText::new("📦").size(60.0));
                                                    }
                                                });
                                            });
                                        });

                                        separator_colored(ui, separator_color, 2.0);

                                        ui.vertical(|ui|
                                        {
                                            apply_available_size(ui);

                                            egui::Frame::default().outer_margin(margin).show(ui, |ui|
                                            {
                                                if is_being_dragged
                                                {
                                                    ui.label(RichText::new(name).color(egui::Color32::WHITE));
                                                }
                                                else
                                                {
                                                    ui.label(name);
                                                }
                                            });
                                        });
                                    });
                                });
                            });
                        });
                    });
                }
            });
        });
    });
}