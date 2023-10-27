use egui::{Ui, ScrollArea, Id, Color32, RichText};

use crate::{state::{state::State, gui::helper::generic_items::drag_item}, helper::file};

use super::editor_state::EditorState;

pub fn create_asset_list(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.set_min_height(200.0);

    let mut asset_list = vec![];

    asset_list.push("objects/grid/grid.gltf");
    asset_list.push("objects/sphere/sphere.gltf");
    asset_list.push("objects/monkey/monkey.gltf");
    asset_list.push("objects/monkey/seperate/monkey.gltf");
    asset_list.push("objects/monkey/monkey.glb");
    asset_list.push("objects/temp/Corset.glb");
    asset_list.push("objects/temp/DamagedHelmet.glb");
    asset_list.push("objects/temp/WaterBottle.glb");
    asset_list.push("objects/temp/MetalRoughSpheres.glb");
    asset_list.push("objects/temp/mando_helmet.glb");
    asset_list.push("objects/temp/mando_helmet_4k.glb");
    asset_list.push("objects/temp/Workbench.glb");
    asset_list.push("objects/temp/Lantern.glb");
    asset_list.push("objects/temp/lotus.glb");
    asset_list.push("objects/temp/Sponza_fixed.glb");
    asset_list.push("objects/temp/scene.glb");
    asset_list.push("objects/temp/model0_debug.glb");
    asset_list.push("objects/temp/Toys_Railway.glb");
    asset_list.push("objects/temp/Toys_Railway_2.glb");
    asset_list.push("objects/temp/test.glb");
    asset_list.push("objects/bastl/bastl.obj");
    asset_list.push("objects/temp/brick_wall.glb");
    asset_list.push("objects/temp/textured.glb");
    asset_list.push("objects/temp/apocalyptic_city.glb");
    asset_list.push("objects/temp/ccity_building_set_1.glb");
    asset_list.push("objects/temp/persian_city.glb");
    asset_list.push("objects/temp/cathedral.glb");
    asset_list.push("objects/temp/minecraft_village.glb");
    asset_list.push("objects/temp/plaza_night_time.glb");
    asset_list.push("objects/temp/de_dust.glb");
    asset_list.push("objects/temp/de_dust2.glb");
    asset_list.push("objects/temp/de_dust2_8k.glb"); // https://sketchfab.com/3d-models/de-dust-2-with-real-light-4ce74cd95c584ce9b12b5ed9dc418db5
    asset_list.push("objects/temp/bistro.glb");
    asset_list.push("objects/temp/lowpoly__fps__tdm__game__map.glb");

    ScrollArea::vertical().show(ui, |ui|
    {
        ui.set_min_width(ui.available_width());
        ui.set_max_width(ui.available_width());

        ui.horizontal_wrapped(|ui|
        {
            //for i in 0..500
            for path in &asset_list
            {
                let str_id = format!("{} asset", path);
                let item_id = Id::new(str_id.clone());
                let name = file::get_stem(path);

                let is_being_dragged = ui.memory(|mem| mem.is_being_dragged(item_id));

                if is_being_dragged
                {
                    editor_state.drag_id = Some(path.to_string());
                }

                ui.allocate_ui(egui::Vec2::new(100.0, 120.0), |ui|
                {
                    drag_item(ui, item_id, |ui|
                    {
                        ui.set_min_width(100.0);
                        ui.set_max_width(100.0);
                        ui.set_min_height(120.0);
                        ui.set_max_height(120.0);

                        let bg_color = Color32::from_white_alpha(3);
                        let mut frame = egui::Frame::group(ui.style()).fill(bg_color);
                        frame.inner_margin = egui::Margin::same(2.0);

                        frame.show(ui, |ui|
                        {
                            ui.vertical_centered(|ui|
                            {
                                ui.label(name);
                                ui.label(RichText::new("📦").size(60.0));
                                ui.label("");
                            });
                        });
                    });
                });
            }
        });
    });
}