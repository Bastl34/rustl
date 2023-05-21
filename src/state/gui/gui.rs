use egui::{FullOutput, RichText, Color32};
use nalgebra::Vector3;

use crate::{state::state::State, rendering::egui::EGui};


pub fn build_gui(state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
{
    let raw_input = egui.ui_state.take_egui_input(window);

    let full_output = egui.ctx.run(raw_input, |ctx|
    {
        egui::Window::new("Settings").show(ctx, |ui|
        {
            ui.label(format!("fps: {}", state.last_fps));
            ui.label("clear color:");

            ui.horizontal(|ui|
            {
                let r = (state.clear_color.x * 255.0) as u8;
                let g = (state.clear_color.y * 255.0) as u8;
                let b = (state.clear_color.z * 255.0) as u8;
                let mut color = Color32::from_rgb(r, g, b);

                ui.label("clear color:");
                let changed = ui.color_edit_button_srgba(&mut color).changed();

                if changed
                {
                    let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                    let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                    let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                    state.clear_color = Vector3::<f32>::new(r, g, b);
                }
            });

            ui.label("fov:");
            ui.add(egui::Slider::new(&mut state.cam_fov, 0.0..=90.0));

            ui.checkbox(&mut state.fullscreen, "Fullscreen");

            ui.label("instances:");
            ui.add(egui::Slider::new(&mut state.instances, 1..=10));

            ui.label("rotation speed:");
            ui.add(egui::Slider::new(&mut state.rotation_speed, 0.0..=0.4));

            ui.horizontal(|ui|
                {
                    let r = (state.light_color.x * 255.0) as u8;
                    let g = (state.light_color.y * 255.0) as u8;
                    let b = (state.light_color.z * 255.0) as u8;
                    let mut color = Color32::from_rgb(r, g, b);

                    ui.label("light color:");
                    let changed = ui.color_edit_button_srgba(&mut color).changed();

                    if changed
                    {
                        let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                        let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                        let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                        state.light_color = Vector3::<f32>::new(r, g, b);
                    }
                });
            // just some tests
            ui.horizontal(|ui|
            {
                ui.selectable_value(& mut state.fullscreen, true, RichText::new("⛶").size(20.0));
                ui.selectable_value(& mut state.fullscreen, false, RichText::new("↕").size(20.0));
            });

            if ui.button("save image").clicked()
            {
                state.save_image = true;
            }

            if ui.button("save depth pass image").clicked()
            {
                state.save_depth_pass_image = true;
            }

            if ui.button("save depth buffer image").clicked()
            {
                state.save_depth_buffer_image = true;
            }

            if ui.button("save screenshot").clicked()
            {
                state.save_screenshot = true;
            }
        });
    });

    let platform_output = full_output.platform_output.clone();

    egui.ui_state.handle_platform_output(window, &egui.ctx, platform_output);

    full_output
}
