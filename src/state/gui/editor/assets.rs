use egui::{Ui, ScrollArea, Id, Color32, RichText};

use crate::{state::{state::State, gui::helper::generic_items::drag_item}, helper::file};

use super::editor_state::{EditorState, AssetType};

pub fn create_asset_section(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.set_min_height(200.0);

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
    ScrollArea::vertical().show(ui, |ui|
    {
        ui.set_min_width(ui.available_width());
        ui.set_max_width(ui.available_width());

        ui.horizontal_wrapped(|ui|
        {
            for path in &editor_state.assets
            {
                let str_id = format!("{} asset", path);
                let item_id = Id::new(str_id.clone());
                let name = file::get_stem(path);

                let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(item_id));

                if is_being_dragged
                {
                    editor_state.drag_id = Some(path.to_string());
                }

                ui.allocate_ui(egui::Vec2::new(100.0, 130.0), |ui|
                {
                    drag_item(ui, item_id, |ui|
                    {
                        ui.set_min_width(100.0);
                        ui.set_max_width(100.0);
                        ui.set_min_height(130.0);
                        ui.set_max_height(130.0);

                        let bg_color = Color32::from_white_alpha(3);
                        let mut frame = egui::Frame::group(ui.style()).fill(bg_color);
                        frame.inner_margin = egui::Margin::same(2.0);

                        let frame = frame.show(ui, |ui|
                        {
                            ui.vertical_centered(|ui|
                            {
                                ui.vertical_centered(|ui|
                                {
                                    ui.set_min_height(50.0);
                                    ui.label(name);
                                });

                                ui.label(RichText::new("📦").size(50.0));
                                ui.label("");
                            });
                        });

                        frame.response.on_hover_text_at_pointer("drag into the scene to load");
                    });
                });
            }
        });
    });
}