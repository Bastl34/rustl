use egui::{Ui, Color32};
use nalgebra::Vector3;

use crate::state::{state::State, gui::helper::generic_items::collapse_with_title};

use super::editor_state::EditorState;

pub fn create_rendering_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // general rendering settings
    collapse_with_title(ui, "render_settings", true, "General Settings", |ui|
    {
        ui.horizontal(|ui|
        {
            let clear_color = state.rendering.clear_color.get_ref();

            let r = (clear_color.x * 255.0) as u8;
            let g = (clear_color.y * 255.0) as u8;
            let b = (clear_color.z * 255.0) as u8;
            let mut color = Color32::from_rgb(r, g, b);

            ui.label("clear color:");
            let changed = ui.color_edit_button_srgba(&mut color).changed();

            if changed
            {
                let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                state.rendering.clear_color.set(Vector3::<f32>::new(r, g, b));
            }
        });

        {
            let mut fullscreen = state.rendering.fullscreen.get_ref().clone();
            if ui.checkbox(&mut fullscreen, "Fullscreen").changed()
            {
                state.rendering.fullscreen.set(fullscreen);
            }
        }

        {
            let mut v_sync = state.rendering.v_sync.get_ref().clone();
            if ui.checkbox(&mut v_sync, "vSync").changed()
            {
                state.rendering.v_sync.set(v_sync);
            }
        }

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut state.rendering.create_mipmaps, "create mipmaps");
            ui.label("ℹ").on_hover_text("applied only for new loaded objects");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut state.rendering.distance_sorting, "Distance Sorting");
            ui.label("ℹ").on_hover_text("for better alpha blending");
        });

        ui.horizontal(|ui|
        {
            ui.label("MSAA:");

            let mut changed = false;
            let mut msaa = *state.rendering.msaa.get_ref();

            changed = ui.selectable_value(& mut msaa, 1, "1").changed() || changed;

            if state.adapter.max_msaa_samples >= 2 { changed = ui.selectable_value(& mut msaa, 2, "2").changed() || changed; }
            if state.adapter.max_msaa_samples >= 4 { changed = ui.selectable_value(& mut msaa, 4, "4").changed() || changed; }
            if state.adapter.max_msaa_samples >= 8 { changed = ui.selectable_value(& mut msaa, 8, "8").changed() || changed; }
            if state.adapter.max_msaa_samples >= 16 { changed = ui.selectable_value(& mut msaa, 16, "16").changed() || changed; }

            if changed
            {
                state.rendering.msaa.set(msaa)
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Max Texture Res:");

            let max = state.adapter.max_texture_resolution;
            let mut current = if let Some(max_texture_resolution) = state.rendering.max_texture_resolution { max_texture_resolution } else { max };

            let mut possibilities = vec![];

            let mut item = max;
            while item > 0
            {
                possibilities.push(item);
                item /= 2;
            }

            let mut changed = false;
            egui::ComboBox::from_label("px").selected_text(format!("{current:?}")).show_ui(ui, |ui|
            {
                ui.style_mut().wrap = Some(false);
                //ui.set_min_width(60.0);

                for item in possibilities
                {
                    changed = ui.selectable_value(&mut current, item, format!("{item:?}")).changed() || changed;
                }
            });

            ui.label("ℹ").on_hover_text("larger textures will be scaled down");

            if changed
            {
                state.rendering.max_texture_resolution = Some(current);
            }
        ui.end_row();
        });
    });
}