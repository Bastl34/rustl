use crate::{gui::{editor::ui::helper::ui_helper::fit_size, helper::generic_items::modal_with_title}, state::state::State};

use super::super::editor_state::EditorState;

pub fn create_modals(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    if editor_state.dialog_add_component
    {
        create_component_add_modal(editor_state, state, ctx);
    }
    else if editor_state.dialog_add_camera_controller
    {
        create_camera_controller_modal(editor_state, state, ctx);
    }
    else if editor_state.dialog_add_scene_controller
    {
        create_scene_controller_modal(editor_state, state, ctx);
    }
    else if editor_state.dialog_debug_image
    {
        create_debug_image_modal(editor_state, state, ctx);
    }
}

pub fn create_component_add_modal(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
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

pub fn create_camera_controller_modal(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
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

pub fn create_scene_controller_modal(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
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

pub fn create_debug_image_modal(editor_state: &mut EditorState, _state: &mut State, ctx: &egui::Context)
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