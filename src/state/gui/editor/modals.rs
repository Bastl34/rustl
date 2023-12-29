use crate::state::{state::State, gui::helper::generic_items::modal_with_title};

use super::editor_state::EditorState;

pub fn create_component_add_modal(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let mut dialog_add_component = editor_state.dialog_add_component;

    modal_with_title(ctx, &mut dialog_add_component, "Add component", |ui|
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
                ui.style_mut().wrap = Some(false);
                ui.set_min_width(40.0);

                for (component_id, component) in state.registered_components.iter().enumerate()
                {
                    ui.selectable_value(&mut editor_state.add_component_id, component_id, component.0.clone());
                }
            });
        });
        if ui.button("Add").clicked()
        {
            let (node_id, instance_id) = editor_state.get_object_ids();

            if let (Some(scene_id), Some(node_id)) = (editor_state.selected_scene_id, node_id)
            {
                let component = state.registered_components.get(editor_state.add_component_id).unwrap().clone();

                let scene = state.find_scene_by_id_mut(scene_id).unwrap();
                let node = scene.find_node_by_id(node_id).unwrap();


                if let Some(instance_id) = instance_id
                {
                    let node = node.read().unwrap();
                    let instance = node.find_instance_by_id(instance_id).unwrap();
                    let mut instance = instance.write().unwrap();
                    let id = scene.id_manager.write().unwrap().get_next_instance_id();
                    instance.add_component(component.1(id, editor_state.add_component_name.as_str()));
                }
                else
                {
                    let id = scene.id_manager.write().unwrap().get_next_instance_id();
                    node.write().unwrap().add_component(component.1(id, editor_state.add_component_name.as_str()));
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