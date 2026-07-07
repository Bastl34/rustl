use egui::{Color32, RichText, Ui};
use nalgebra::Vector3;

use crate::{component_downcast, gui::helper::generic_items::collapse_with_title, rendering::morph_target::MorphTarget, state::{helper::render_item::{get_render_item, render_item_gpu_usage}, scene::{components::{material::Material, mesh::Mesh}, scene::Scene}, state::{PresentModeSetting, State}}};

use super::super::editor_state::EditorState;

pub fn create_general_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    create_rendering_settings(editor_state, state, ui);
    create_audio_settings(editor_state, state, ui);
}

pub fn create_rendering_settings(_editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // general statistics
    let mut instances_amout = 0;
    let mut meshes_amout = 0;
    let mut nodes_solid_amout = 0;
    let mut nodes_transparent_amout = 0;

    let mut materials_amout = 0;
    let mut cameras_amout = 0;
    let mut lights_amout = 0;

    let mut nodes_amount = 0;
    let mut vertices_amout = 0;
    let mut indices_amout = 0;

    let mut buffer_gpu_memory_usage: u64 = 0;
    let mut morph_target_tex_gpu_memory_usage: u64 = 0;

    for scene in &state.scenes
    {
        let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

        for node in &all_nodes
        {
            let node = node.read().unwrap();
            instances_amout += node.instances.get_ref().len();

            if let Some(material) = node.find_component::<Material>()
            {
                component_downcast!(material, Material);
                if material.has_transparency()
                {
                    nodes_transparent_amout += 1;
                }
                else
                {
                    nodes_solid_amout += 1;
                }
            }

            // per-node buffers (instances, skeleton, morph target bind group)
            buffer_gpu_memory_usage += render_item_gpu_usage(&node.instance_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&node.skeleton_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&node.skeleton_morph_target_bind_group_render_item);

            if let Some(mesh) = node.find_component::<Mesh>()
            {
                component_downcast!(mesh, Mesh);
                buffer_gpu_memory_usage += render_item_gpu_usage(&mesh.morph_target_render_item);

                if let Some(render_item) = mesh.morph_target_render_item.as_ref()
                {
                    let morph_target = get_render_item::<MorphTarget>(render_item);
                    morph_target_tex_gpu_memory_usage += morph_target.texture_gpu_usage();
                }
            }
        }

        nodes_amount += all_nodes.len();

        materials_amout += scene.materials.len();
        cameras_amout += scene.cameras.len();
        lights_amout += scene.lights.get_ref().len();

        // scene level buffers (rendering scene internals + lights)
        buffer_gpu_memory_usage += render_item_gpu_usage(&scene.render_item);
        buffer_gpu_memory_usage += render_item_gpu_usage(&scene.lights_render_item);

        // camera buffers
        for camera in &scene.cameras
        {
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.bind_group_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.hzb_texture_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.hzb_downsample_bind_group_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.visibility_buffer_render_item);
            buffer_gpu_memory_usage += render_item_gpu_usage(&camera.hzb_occlusion_bind_group_render_item);
        }

        // material buffers
        for material in scene.materials.values()
        {
            let material = material.read().unwrap();
            buffer_gpu_memory_usage += render_item_gpu_usage(&material.get_base().render_item);
        }
    }

    meshes_amout += state.resources.mesh_resources.len();
    for mesh_resource in &state.resources.mesh_resources
    {
        let mesh_resource = mesh_resource.1.read().unwrap();
        vertices_amout += mesh_resource.get_data().vertices.len();
        indices_amout += mesh_resource.get_data().indices.len();

        // vertex / index buffers
        buffer_gpu_memory_usage += render_item_gpu_usage(&mesh_resource.render_item);
    }

    let buffer_gpu_memory_usage = buffer_gpu_memory_usage as f32 / 1024.0 / 1024.0;

    let mut tex_memory_usage = 0.0;
    let mut tex_gpu_memory_usage = 0.0;
    for texture in &state.resources.textures
    {
        let texture = texture.1.as_ref().read().unwrap();
        let texture = texture.as_ref();
        tex_memory_usage += texture.memory_usage() as f32;
        tex_gpu_memory_usage += texture.gpu_usage() as f32;
    }

    // morph target textures are GPU textures too
    tex_gpu_memory_usage += morph_target_tex_gpu_memory_usage as f32;

    tex_memory_usage = tex_memory_usage / 1024.0 / 1024.0;
    tex_gpu_memory_usage = tex_gpu_memory_usage / 1024.0 / 1024.0;

    // statistics
    collapse_with_title(ui, "general_info", true, "📈 Info", None, |ui|
    {
        ui.label(RichText::new("🎬 scenes").strong());
        ui.label(format!(" ⚫ scenes: {}", state.scenes.len()));
        ui.label(format!(" ⚫ nodes: {}", nodes_amount));

        ui.horizontal(|ui|
        {
            ui.add_space(16.0);
            ui.vertical(|ui|
            {
                ui.label(format!(" ⚫ solid: {}", nodes_solid_amout));
                ui.label(format!(" ⚫ transparent: {}", nodes_transparent_amout));
            });
        });
        ui.label(format!(" ⚫ instances: {}", instances_amout));
        ui.label(format!(" ⚫ materials: {}", materials_amout));
        ui.label(format!(" ⚫ textures: {}", state.resources.textures.len()));
        ui.label(format!(" ⚫ cameras: {}", cameras_amout));
        ui.label(format!(" ⚫ lights: {}", lights_amout));

        ui.label(RichText::new("◼ geometry").strong());
        ui.label(format!(" ⚫ meshes: {}", meshes_amout));
        ui.label(format!(" ⚫ vertices: {}", vertices_amout));
        ui.label(format!(" ⚫ indices: {}", indices_amout));

        ui.label(RichText::new("🖴 RAM memory usage").strong());
        ui.label(format!(" ⚫ textures: {:.2} MB", tex_memory_usage));

        ui.label(RichText::new("🖵 GPU memory usage").strong());
        ui.label(format!(" ⚫ textures: {:.2} MB", tex_gpu_memory_usage));
        ui.label(format!(" ⚫ buffers: {:.2} MB", buffer_gpu_memory_usage));
    });

    // general rendering settings
    collapse_with_title(ui, "render_settings", true, "📷 Rendering Settings", None, |ui|
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
            let mut present_mode = state.rendering.present_mode.get_ref().clone();
            let label = |m: PresentModeSetting| match m
            {
                PresentModeSetting::VSync     => "VSync",
                PresentModeSetting::FastVSync => "Fast VSync",
                PresentModeSetting::VSyncOff  => "VSync Off",
            };
            ui.horizontal(|ui|
            {
                ui.label("Present mode:");
                egui::ComboBox::from_id_salt("present_mode_combo")
                    .selected_text(label(present_mode))
                    .show_ui(ui, |ui|
                    {
                        let mut changed = false;
                        for m in [PresentModeSetting::VSync, PresentModeSetting::FastVSync, PresentModeSetting::VSyncOff]
                        {
                            if ui.selectable_value(&mut present_mode, m, label(m)).changed()
                            {
                                changed = true;
                            }
                        }
                        if changed
                        {
                            state.rendering.present_mode.set(present_mode);
                        }
                    });
            });
        }

        ui.horizontal(|ui|
        {
            ui.add_enabled(state.rendering_adapter.wireframe_mode_support, egui::Checkbox::new(&mut state.rendering.wireframe_mode, "Wireframe Mode"));
            ui.label("ℹ").on_hover_text(if state.rendering_adapter.wireframe_mode_support { "renders all objects in wireframe mode, useful for debugging" } else { "not supported by this GPU/backend" });
        });

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
            ui.checkbox(&mut state.rendering.frustum_culling, "Frustum Culling");
            ui.label("ℹ").on_hover_text("improves performance by not rendering objects outside the view frustum");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut state.rendering.occlusion_culling, "Occlusion Culling");
            ui.label("ℹ").on_hover_text("improves performance by not rendering objects which are occluded by other objects in the view frustum");
        });

        ui.horizontal(|ui|
        {
            ui.label("MSAA:");

            let mut changed = false;
            let mut msaa = *state.rendering.msaa.get_ref();

            changed = ui.selectable_value(& mut msaa, 1, "1").changed() || changed;

            if state.rendering_adapter.max_msaa_samples >= 2 { changed = ui.selectable_value(& mut msaa, 2, "2").changed() || changed; }
            if state.rendering_adapter.max_msaa_samples >= 4 { changed = ui.selectable_value(& mut msaa, 4, "4").changed() || changed; }
            if state.rendering_adapter.max_msaa_samples >= 8 { changed = ui.selectable_value(& mut msaa, 8, "8").changed() || changed; }
            if state.rendering_adapter.max_msaa_samples >= 16 { changed = ui.selectable_value(& mut msaa, 16, "16").changed() || changed; }

            if changed
            {
                state.rendering.msaa.set(msaa)
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Max Texture Res:");

            let max = state.rendering_adapter.max_texture_resolution;
            let mut current = if let Some(max_texture_resolution) = state.rendering.max_texture_resolution { max_texture_resolution } else { max };

            let mut possibilities = vec![];

            let mut item = max;
            while item > 0
            {
                possibilities.push(item);
                item /= 2;
            }

            let mut changed = false;
            egui::ComboBox::from_id_salt("max_texture_res_combo").selected_text(format!("{current:?}")).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                //ui.set_min_width(60.0);

                for item in possibilities
                {
                    changed = ui.selectable_value(&mut current, item, format!("{item:?}")).changed() || changed;
                }
            });
            ui.label("px");

            ui.label("ℹ").on_hover_text("larger textures will be scaled down");

            if changed
            {
                state.rendering.max_texture_resolution = Some(current);
            }
            ui.end_row();
        });

        ui.horizontal(|ui|
        {
            let mut shadow_enabled = *state.rendering.shadow.get_ref();
            if ui.checkbox(&mut shadow_enabled, "Shadows").changed()
            {
                *state.rendering.shadow.get_mut() = shadow_enabled;
            }
            ui.end_row();
        });

        ui.horizontal(|ui|
        {
            ui.label("Shadow Map Res:");

            let max = state.rendering_adapter.max_texture_resolution;
            let mut current = *state.rendering.shadow_map_resolution.get_ref();

            let mut possibilities = vec![];

            let mut item = max;
            while item > 0
            {
                possibilities.push(item);
                item /= 2;
            }

            let mut changed = false;
            egui::ComboBox::from_id_salt("shadow_map_res_combo").selected_text(format!("{current:?}")).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                //ui.set_min_width(60.0);

                for item in possibilities
                {
                    changed = ui.selectable_value(&mut current, item, format!("{item:?}")).changed() || changed;
                }
            });
            ui.label("px");

            ui.label("ℹ").on_hover_text("larger shadow map sizes can impact performance");

            if changed
            {
                *state.rendering.shadow_map_resolution.get_mut() = current;
            }
            ui.end_row();
        });

        ui.horizontal(|ui|
        {
            ui.label("Shadow Distance:");

            let mut shadow_max_distance = state.rendering.shadow_max_distance;
            if ui.add(egui::DragValue::new(&mut shadow_max_distance).speed(1.0).range(1.0..=10000.0)).changed()
            {
                state.rendering.shadow_max_distance = shadow_max_distance;
            }

            ui.label("ℹ").on_hover_text("max distance (from the camera) covered by directional light shadows (cascades) - smaller = sharper shadows");

            ui.end_row();
        });
    });
}

pub fn create_audio_settings(_editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // general rendering settings
    collapse_with_title(ui, "audio_settings", true, "🔊 Audio Settings", None, |ui|
    {
        let mut volume;

        {
            let audio_device = state.io.audio_device.read().unwrap();
            let audio_device_data = audio_device.data.get_ref();

            volume = audio_device_data.volume;
        }

        let mut changed = false;

        changed = ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).text("Global Volume")).changed() || changed;

        if changed
        {
            let mut audio_device = state.io.audio_device.write().unwrap();
            let audio_device_data = audio_device.data.get_mut();

            audio_device_data.volume = volume;
        }
    });
}