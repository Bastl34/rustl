use egui::{Ui, ScrollArea, Id, Color32, RichText, ViewportId};

use crate::{gui::helper::generic_items::separator_colored, helper::platform::is_mac, state::state::State};

use super::super::editor_state::{EditorState, AssetType, Asset, BottomPanel};

const TILE_WIDTH: f32 = 100.0;
const TILE_HEIGHT: f32 = 150.0;
const TILE_MARGIN: f32 = 2.0;

pub fn assets_viewport_id() -> ViewportId
{
    ViewportId::from_hash_of("assets_window")
}

pub fn create_asset_section(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.set_min_height(220.0);

    ui.horizontal_top(|ui|
    {
        create_asset_tree(editor_state, state, ui);
        create_asset_list(editor_state, state, ui);
    });
}

pub fn create_asset_tree(editor_state: &mut EditorState, _state: &mut State, ui: &mut Ui)
{
    ui.scope(|ui|
    {
        ui.set_min_width(100.0);
        //ui.set_max_width(100.0);

        ui.vertical(|ui|
        {
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Scene, "🎬 Scenes");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Object, "📦 Objects");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Texture, "🖼 Textures");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Material, "🎨 Materials");
            ui.selectable_value(&mut editor_state.asset_type, AssetType::Sound, "🔊 Sounds").on_hover_text("drag a sound into the editor to add it to the sound resources");
        });
    });
}

pub fn create_asset_list(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let items = match editor_state.asset_type
    {
        AssetType::Scene => Some(&editor_state.assets_scenes),
        AssetType::Object => Some(&editor_state.assets_objects),
        AssetType::Texture => None,
        AssetType::Material => Some(&editor_state.assets_materials),
        AssetType::Sound => Some(&editor_state.assets_sounds),
    };

    if items.is_none() { return; }
    let items = items.unwrap();

    let mut reload_assets = false;
    let mut dock_assets_window = false;

    ui.vertical(|ui|
    {
        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut editor_state.asset_filter).desired_width(100.0));

            if editor_state.asset_type == AssetType::Object || editor_state.asset_type == AssetType::Material
            {
                ui.checkbox(&mut editor_state.reuse_materials_by_name, "Reuse Materials by name");
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                // shown in the assets window
                if ui.ctx().viewport_id() == assets_viewport_id()
                {
                    dock_assets_window = ui.button("⬋").on_hover_text("dock the assets back into the bottom panel").clicked();
                }

                if ui.button("⟳").clicked()
                {
                    reload_assets = true;
                }

                if editor_state.asset_type == AssetType::Material
                {
                    let running = *editor_state.material_thumbnails_running.read().unwrap();
                    if ui.add_enabled(!running, egui::Button::new("🖼 Create Thumbnails")).on_hover_text("render preview thumbnails for materials without one").clicked()
                    {
                        editor_state.generate_material_thumbnails = Some(true);
                    }
                }
            });
        });

        ScrollArea::vertical().show(ui, |ui|
        {
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());

            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui|
            {
                ui.spacing_mut().item_spacing = egui::Vec2::new(2.0, 2.0);
                for asset in items
                {
                    let filter = editor_state.asset_filter.to_lowercase();

                    if !filter.is_empty() && asset.name.to_lowercase().find(filter.as_str()).is_none()
                    {
                        continue;
                    }

                    let str_id = format!("{} asset", asset.path);
                    let item_id = Id::unique(str_id.clone());
                    let str_id_inner = format!("{}_inner", str_id.clone());

                    // if is_being_dragged
                    let is_being_dragged = ui.ctx().is_being_dragged(item_id);
                    if is_being_dragged
                    {
                        editor_state.drag_id = Some(asset.path.clone());
                        editor_state.drag_viewport = ui.ctx().viewport_id();
                    }

                    ui.allocate_ui(egui::Vec2::new(TILE_WIDTH + (TILE_MARGIN * 2.0), TILE_HEIGHT + (TILE_MARGIN * 2.0)), |ui|
                    {
                        apply_available_size(ui);

                        ui.dnd_drag_source(item_id, asset.path.clone(), |ui|
                        {
                            apply_available_size(ui);

                            ui.push_id(str_id_inner, |ui|
                            {
                                apply_available_size(ui);
                                create_asset_tile(ui, asset, is_being_dragged);
                            });
                        });
                    });
                }
            });
        });
    });

    if reload_assets
    {
        editor_state.load_all_asset_entries(state, ui.ctx());
    }

    if dock_assets_window
    {
        dock_assets(editor_state);
    }
}

fn apply_available_size(ui: &mut Ui)
{
    ui.set_min_width(ui.available_width());
    ui.set_max_width(ui.available_width());
    ui.set_min_height(ui.available_height());
    ui.set_max_height(ui.available_height());
}

// preview image + name
fn create_asset_tile(ui: &mut Ui, asset: &Asset, highlighted: bool)
{
    let image_size = TILE_WIDTH - 20.0;

    let bg_color = Color32::from_white_alpha(3);
    let highlight_color = egui::Color32::from_rgba_premultiplied(0, 100, 210, 50);
    let separator_color = Color32::LIGHT_GRAY;
    let image_background_color = Color32::from_rgba_premultiplied(0, 0, 0, 150);

    let shadow = egui::Shadow
    {
        offset: [2, 2].into(),
        blur: 4,
        spread: 2,
        color: egui::Color32::from_black_alpha(180),
        //color: egui::Color32::from_white_alpha(180)
    };

    let apply_size = |ui: &mut Ui|
    {
        ui.set_min_width(TILE_WIDTH);
        ui.set_max_width(TILE_WIDTH);
        ui.set_min_height(TILE_HEIGHT);
        ui.set_max_height(TILE_HEIGHT);
    };

    let stroke_color = if highlighted { highlight_color } else { Color32::TRANSPARENT };
    let fill_color = if highlighted { highlight_color } else { bg_color };
    let frame = egui::Frame::default().fill(fill_color).corner_radius(2.0).shadow(shadow).outer_margin(TILE_MARGIN).stroke(egui::Stroke::new(2.0, stroke_color));

    frame.show(ui, |ui|
    {
        ui.style_mut().interaction.selectable_labels = false;
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
                        else if asset.asset_type == AssetType::Scene
                        {
                            ui.label(RichText::new("🎬").size(60.0));
                        }
                        else if asset.asset_type == AssetType::Object
                        {
                            ui.label(RichText::new("📦").size(60.0));
                        }
                        else if asset.asset_type == AssetType::Texture
                        {
                            ui.label(RichText::new("🖼").size(60.0));
                        }
                        else if asset.asset_type == AssetType::Material
                        {
                            ui.label(RichText::new("🎨").size(60.0));
                        }
                        else if asset.asset_type == AssetType::Sound
                        {
                            ui.label(RichText::new("🔊").size(60.0));
                        }
                    });
                });
            });

            separator_colored(ui, separator_color, 2.0);

            ui.vertical(|ui|
            {
                apply_available_size(ui);

                egui::Frame::default().outer_margin(TILE_MARGIN).show(ui, |ui|
                {
                    if highlighted
                    {
                        ui.label(RichText::new(&asset.name).color(egui::Color32::WHITE));
                    }
                    else
                    {
                        ui.label(&asset.name);
                    }
                });
            });
        });
    });
}

// back into the bottom panel (and show them there)
pub fn dock_assets(editor_state: &mut EditorState)
{
    editor_state.assets_window_open = false;
    editor_state.bottom = BottomPanel::Assets;
    editor_state.bottom_panel_open = true;
}

// the assets in their own native window - called at the end of the main window ui (the drop check needs its free rect)
pub fn create_assets_window(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    if !editor_state.assets_window_supported || !editor_state.assets_window_open
    {
        editor_state.assets_window_open = false;
        cancel_asset_window_drag(editor_state);
        return;
    }

    let ctx = ui.ctx().clone();
    let viewport_id = assets_viewport_id();

    let mut builder = egui::ViewportBuilder::default().with_title("Assets").with_min_inner_size([420.0, 220.0]);

    // reopens where it was closed - the first time over the lower part of the main window
    if let (Some(pos), Some(size)) = (editor_state.assets_window_pos, editor_state.assets_window_size)
    {
        builder = builder.with_position(pos).with_inner_size(size);
    }
    else
    {
        let size = egui::vec2(960.0, 380.0);
        builder = builder.with_inner_size(size);

        if let Some(main_rect) = ctx.input(|i| i.viewport().inner_rect)
        {
            builder = builder.with_position(main_rect.left_bottom() + egui::vec2(40.0, -size.y - 80.0));
        }
    }

    let drop = ctx.show_viewport_immediate(viewport_id, builder, |ui, _class|
    {
        if ui.input(|i| i.viewport().close_requested())
        {
            editor_state.assets_window_open = false;
        }

        if let Some((outer_rect, inner_rect)) = ui.input(|i| i.viewport().outer_rect.zip(i.viewport().inner_rect))
        {
            editor_state.assets_window_pos = Some(outer_rect.min);
            editor_state.assets_window_size = Some(inner_rect.size());
        }

        egui::CentralPanel::default().show(ui, |ui|
        {
            // play mode: stays open, but the game has the input
            if !editor_state.visible
            {
                ui.disable();
            }

            create_asset_section(editor_state, state, ui);
        });

        track_asset_window_drag(editor_state, ui.ctx())
    });

    if let Some((path, pos)) = drop
    {
        if is_pos_in_scene_view(ui, pos)
        {
            editor_state.asset_window_drop = Some((path, pos));

            // the next input most likely goes to the scene - unless the focused main window would cover the assets window
            let assets_rect = ctx.input(|i| i.raw.viewports.get(&viewport_id).and_then(|info| info.outer_rect));
            let main_rect = ctx.input(|i| i.viewport().outer_rect);
            if let (Some(assets_rect), Some(main_rect)) = (assets_rect, main_rect) && !assets_rect.intersects(main_rect)
            {
                ctx.send_viewport_cmd_to(ViewportId::ROOT, egui::ViewportCommand::Focus);
            }
        }
    }

    // egui's drag preview ends at the border of the assets window -> continued in the main window
    if let Some(pos) = editor_state.asset_window_drag_pos
    {
        paint_asset_drag_preview(editor_state, ui, pos);
    }

    if !editor_state.assets_window_open
    {
        cancel_asset_window_drag(editor_state);
    }
}

// the os sends all pointer events of a drag to the window it started in -> the pointer is mapped into the main window here
fn track_asset_window_drag(editor_state: &mut EditorState, ctx: &egui::Context) -> Option<(String, egui::Pos2)>
{
    if editor_state.drag_viewport != ctx.viewport_id()
    {
        return None;
    }

    let path = editor_state.drag_id.clone()?;

    // over the assets window itself (it may overlap the main window) nothing happens in the main window
    let pointer = ctx.input(|i| i.pointer.latest_pos().or(i.pointer.interact_pos()));
    let pos_in_main = pointer.filter(|pos| !ctx.viewport_rect().contains(*pos)).and_then(|pos| map_pos_to_viewport(ctx, ViewportId::ROOT, pos));

    if ctx.dragged_id().is_some()
    {
        editor_state.asset_window_drag_pos = pos_in_main;
        return None;
    }

    // released
    editor_state.drag_id = None;
    editor_state.asset_window_drag_pos = None;

    pos_in_main.map(|pos| (path, pos))
}

fn cancel_asset_window_drag(editor_state: &mut EditorState)
{
    if editor_state.drag_viewport != ViewportId::ROOT
    {
        editor_state.drag_id = None;
        editor_state.drag_viewport = ViewportId::ROOT;
    }

    editor_state.asset_window_drag_pos = None;
}

// maps a position (ui points) of the current viewport into another one - None without window positions
fn map_pos_to_viewport(ctx: &egui::Context, target: ViewportId, pos: egui::Pos2) -> Option<egui::Pos2>
{
    let zoom_factor = ctx.zoom_factor();
    let (from, to) = ctx.input(|i| (i.viewport().clone(), i.raw.viewports.get(&target).cloned()));
    let to = to?;

    let from_rect = from.inner_rect?;
    let to_rect = to.inner_rect?;

    // macos: window positions are in points on every monitor, otherwise in physical pixels (differs for monitors with other scalings)
    if is_mac()
    {
        return Some(pos + (from_rect.min - to_rect.min));
    }

    let from_pixels_per_point = from.native_pixels_per_point? * zoom_factor;
    let to_pixels_per_point = to.native_pixels_per_point? * zoom_factor;
    let pos_px = (from_rect.min.to_vec2() + pos.to_vec2()) * from_pixels_per_point;

    Some((pos_px / to_pixels_per_point - to_rect.min.to_vec2()).to_pos2())
}

// the 3d view is what the panels leave free (same check as egui's is_pointer_over_egui, but for any position)
fn is_pos_in_scene_view(ui: &Ui, pos: egui::Pos2) -> bool
{
    if !ui.available_rect_before_wrap().contains(pos)
    {
        return false;
    }

    match ui.ctx().layer_id_at(pos)
    {
        Some(layer_id) => layer_id.order == egui::Order::Background,
        None => true,
    }
}

fn paint_asset_drag_preview(editor_state: &EditorState, ui: &Ui, pos: egui::Pos2)
{
    let asset = editor_state.drag_id.as_ref().and_then(|path|
    {
        [&editor_state.assets_scenes, &editor_state.assets_objects, &editor_state.assets_materials, &editor_state.assets_sounds].into_iter().flatten().find(|asset| &asset.path == path)
    });

    let Some(asset) = asset else
    {
        return;
    };

    // dimmed where it can not be dropped
    let opacity = if is_pos_in_scene_view(ui, pos) { 1.0 } else { 0.4 };

    egui::Area::new(Id::unique("asset_window_drag_preview")).order(egui::Order::Tooltip).interactable(false).pivot(egui::Align2::CENTER_CENTER).fixed_pos(pos).show(ui.ctx(), |ui|
    {
        ui.multiply_opacity(opacity);

        ui.allocate_ui(egui::Vec2::new(TILE_WIDTH + (TILE_MARGIN * 2.0), TILE_HEIGHT + (TILE_MARGIN * 2.0)), |ui|
        {
            apply_available_size(ui);
            create_asset_tile(ui, asset, true);
        });
    });
}
