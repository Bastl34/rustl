use egui::{FullOutput, RichText};

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
            ui.add(egui::Slider::new(&mut state.clear_color_r, 0.0..=1.0));
            ui.add(egui::Slider::new(&mut state.clear_color_g, 0.0..=1.0));
            ui.add(egui::Slider::new(&mut state.clear_color_b, 0.0..=1.0));

            ui.label("fov:");
            ui.add(egui::Slider::new(&mut state.cam_fov, 0.0..=90.0));

            ui.checkbox(&mut state.fullscreen, "Fullscreen");

            ui.label("instances:");
            ui.add(egui::Slider::new(&mut state.instances, 1..=10));

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
