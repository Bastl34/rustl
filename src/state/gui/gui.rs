use std::cell::RefCell;

use egui::{FullOutput, RichText, Color32};
use nalgebra::{Vector3, Point3};

use crate::{state::{state::State, scene::{light::Light, camera::Camera}}, rendering::egui::EGui, helper::change_tracker::ChangeTracker};


pub fn build_gui(state: &mut State, window: &winit::window::Window, egui: &mut EGui) -> FullOutput
{
    let raw_input = egui.ui_state.take_egui_input(window);

    let full_output = egui.ctx.run(raw_input, |ctx|
    {
        egui::Window::new("Settings").show(ctx, |ui|
        {
            ui.label(format!("fps: {}", state.last_fps));
            ui.label(format!("absolute fps: {}", state.fps_absolute));
            ui.label(format!("draw calls: {}", state.draw_calls));
            ui.label(format!("frame time: {:.3} ms", state.frame_time));
            ui.label(format!("update time: {:.3} ms", state.update_time));
            ui.label(format!("render time: {:.3} ms", state.render_time));

            ui.horizontal(|ui|
            {
                let r = (state.rendering.clear_color.x * 255.0) as u8;
                let g = (state.rendering.clear_color.y * 255.0) as u8;
                let b = (state.rendering.clear_color.z * 255.0) as u8;
                let mut color = Color32::from_rgb(r, g, b);

                ui.label("clear color:");
                let changed = ui.color_edit_button_srgba(&mut color).changed();

                if changed
                {
                    let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                    let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                    let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);
                    state.rendering.clear_color = Vector3::<f32>::new(r, g, b);
                }
            });
            ui.checkbox(&mut state.rendering.fullscreen, "Fullscreen");

            ui.horizontal(|ui|
            {
                ui.label("MSAA:");

                let mut changed = false;
                let mut msaa = *state.rendering.msaa.get();

                changed = ui.selectable_value(& mut msaa, 1, "1").changed() || changed;

                if state.adapter.msaa_samples >= 2 { changed = ui.selectable_value(& mut msaa, 2, "2").changed() || changed; }
                if state.adapter.msaa_samples >= 4 { changed = ui.selectable_value(& mut msaa, 4, "4").changed() || changed; }
                if state.adapter.msaa_samples >= 8 { changed = ui.selectable_value(& mut msaa, 8, "8").changed() || changed; }
                if state.adapter.msaa_samples >= 16 { changed = ui.selectable_value(& mut msaa, 16, "16").changed() || changed; }

                if changed
                {
                    state.rendering.msaa.set(msaa)
                }
            });

            ui.horizontal(|ui|
            {
                ui.label("instances:");
                ui.add(egui::Slider::new(&mut state.instances, 1..=10000));
            });

            ui.horizontal(|ui|
            {
                ui.label("rotation speed:");
                ui.add(egui::Slider::new(&mut state.rotation_speed, 0.0..=2.0));
            });

            // camera stuff
            let mut cams: Vec<(usize, usize, String, Point3<f32>, f32)> = vec![];

            for (s, scene) in state.scenes.iter().enumerate()
            {
                for (c, cam) in scene.cameras.iter().enumerate()
                {
                    let cam = cam.borrow();
                    let cam = cam.get_ref();

                    cams.push((s, c, cam.name.clone(), cam.eye_pos.clone(), cam.fovy));
                }
            }

            for cam in cams.iter_mut()
            {
                let (scene_id, cam_id, name, pos, mut fov) = cam;

                fov = fov.to_degrees();

                ui.horizontal(|ui|
                {
                    let mut changed = false;

                    ui.label(name.as_str());
                    changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                    changed = ui.add(egui::Slider::new(&mut fov, 0.0..=90.0)).changed() || changed;

                    if changed
                    {
                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        let cam = scene.cameras.get(cam_id.clone()).unwrap();
                        let mut cam = cam.borrow_mut();
                        let cam = cam.get_mut();

                        cam.eye_pos = pos.clone();
                        cam.fovy = fov.to_radians();
                        cam.init_matrices();
                    }
                });
            }

            let mut lights: Vec<(usize, usize, String, Point3<f32>, Color32)> = vec![];

            for (s, scene) in state.scenes.iter().enumerate()
            {
                for (l, light) in scene.lights.get_ref().iter().enumerate()
                {
                    let light = light.borrow();
                    let light = light.get_ref();

                    let r = (light.color.x * 255.0) as u8;
                    let g = (light.color.y * 255.0) as u8;
                    let b = (light.color.z * 255.0) as u8;
                    let color = Color32::from_rgb(r, g, b);

                    lights.push((s, l, light.name().clone(), light.pos.clone(), color));
                }
            }

            for light in lights.iter_mut()
            {
                let (scene_id, light_id, name, pos, mut color) = light;

                ui.horizontal(|ui|
                {
                    let mut changed = false;

                    ui.label(name.as_str());
                    changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                    changed = ui.color_edit_button_srgba(&mut color).changed() || changed;

                    if changed
                    {
                        let r = ((color.r() as f32) / 255.0).clamp(0.0, 1.0);
                        let g = ((color.g() as f32) / 255.0).clamp(0.0, 1.0);
                        let b = ((color.b() as f32) / 255.0).clamp(0.0, 1.0);

                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        let light = scene.lights.get_ref().get(light_id.clone()).unwrap();
                        let mut light = light.borrow_mut();
                        let light = light.get_mut();
                        light.pos = pos.clone();
                        light.color = Vector3::<f32>::new(r, g, b);
                    }

                    if ui.button("🗑").clicked()
                    {
                        let scene = state.scenes.get_mut(scene_id.clone()).unwrap();
                        scene.lights.get_mut().remove(light_id.clone());
                    }
                });
            }

            ui.horizontal(|ui|
            {
                ui.label("add light: ");
                if ui.button("+").clicked()
                {
                    let scene = state.scenes.get_mut(0).unwrap();

                    let light_id = scene.id_manager.get_next_light_id();
                    let light = Light::new_point(light_id, Point3::<f32>::new(2.0, 5.0, 2.0), Vector3::<f32>::new(1.0, 1.0, 1.0), 1.0);
                    scene.lights.get_mut().push(RefCell::new(ChangeTracker::new(Box::new(light))));
                }
            });

            // just some tests
            ui.horizontal(|ui|
            {
                ui.selectable_value(& mut state.rendering.fullscreen, true, RichText::new("⛶").size(20.0));
                ui.selectable_value(& mut state.rendering.fullscreen, false, RichText::new("↕").size(20.0));
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
