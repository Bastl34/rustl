use egui::Color32;
use nalgebra::Vector3;

use crate::{helper::{change_tracker::ChangeTracker, self}, component_impl_default, state::{scene::{node::{NodeItem, InstanceItemRefCell}, instance::InstanceItem}, gui::info_box::{info_box, success_box, error_box, warn_box}}, component_downcast, component_downcast_mut, input::{input_manager::InputManager, keyboard::{Key, get_keys_as_string_vec}}};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};

const INFO_STRING: &str = "The changes are applies on the Transform Component.\nThey are multiplied by frame_scale for each frame.\nIf there is no Transform Component: Nothing is happening.";

pub struct TransformationAnimationData
{
    pub translation: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scale: Vector3<f32>,
}

pub struct TransformationAnimation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationAnimationData>,

    pub keyboard_key: Option<usize>,
}

impl TransformationAnimation
{
    pub fn new(id: u64, name: &str, translation: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> TransformationAnimation
    {
        let data = TransformationAnimationData
        {
            translation,
            rotation,
            scale
        };

        let mut transform_animation = TransformationAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Transform. Animation".to_string(), "🏃".to_string()),
            data: ChangeTracker::new(data),
            keyboard_key: None
        };

        transform_animation.base.info = Some(INFO_STRING.to_string());

        transform_animation
    }

    pub fn new_empty(id: u64, name: &str) -> TransformationAnimation
    {
        let data = TransformationAnimationData
        {
            translation: Vector3::<f32>::zeros(),
            rotation: Vector3::<f32>::zeros(),
            scale: Vector3::<f32>::zeros()
        };

        let mut transform_animation = TransformationAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Transform. Animation".to_string(), "🏃".to_string()),
            data: ChangeTracker::new(data),
            keyboard_key: None
        };

        transform_animation.base.info = Some(INFO_STRING.to_string());

        transform_animation
    }

    pub fn get_data(&self) -> &TransformationAnimationData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<TransformationAnimationData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<TransformationAnimationData>
    {
        &mut self.data
    }

    fn _update(&mut self, transform_component: Option<ComponentItem>, input_manager: &mut InputManager, frame_scale: f32)
    {
        if let Some(keyboard_key) = self.keyboard_key
        {
            if !input_manager.keyboard.is_holding(Key::from_repr(keyboard_key).unwrap())
            {
                return;
            }
        }

        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);

            let data = self.get_data();
            let mut translation = None;
            let mut rotation = None;
            let mut scale = None;

            if !helper::math::approx_zero(data.translation.x) || !helper::math::approx_zero(data.translation.y) || !helper::math::approx_zero(data.translation.z)
            {
                translation = Some(Vector3::<f32>::new(data.translation.x * frame_scale, data.translation.y * frame_scale, data.translation.z * frame_scale));
            }

            if !helper::math::approx_zero(data.rotation.x) || !helper::math::approx_zero(data.rotation.y) || !helper::math::approx_zero(data.rotation.z)
            {
                rotation = Some(Vector3::<f32>::new(data.rotation.x * frame_scale, data.rotation.y * frame_scale, data.rotation.z * frame_scale));
            }

            if !helper::math::approx_zero(data.scale.x) || !helper::math::approx_zero(data.scale.y) || !helper::math::approx_zero(data.scale.z)
            {
                scale = Some(Vector3::<f32>::new(data.scale.x * frame_scale, data.scale.y * frame_scale, data.scale.z * frame_scale));
            }

            transform_component.apply_transformation(translation, None, rotation);

            if let Some(scale) = scale
            {
                transform_component.apply_scale(scale, false);
            }
        }
    }
}

impl Component for TransformationAnimation
{
    component_impl_default!();

    fn instantiable(&self) -> bool
    {
        true
    }

    fn set_enabled(&mut self, state: bool)
    {
        if self.base.is_enabled != state
        {
            self.base.is_enabled = state;

            // force update
            self.data.force_change();
        }
    }

    fn update(&mut self, node: NodeItem, input_manager: &mut InputManager, frame_scale: f32)
    {
        let node = node.write().unwrap();
        self._update(node.find_component::<Transformation>(), input_manager, frame_scale);
    }

    fn update_instance(&mut self, _node: NodeItem, instance: &InstanceItemRefCell, input_manager: &mut InputManager, frame_scale: f32)
    {
        let instance = instance.borrow();
        self._update(instance.find_component::<Transformation>(), input_manager, frame_scale);
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let mut changed = false;

        let mut trans;
        let mut rot;
        let mut scale;
        {
            let data = self.get_data();

            trans = data.translation;
            rot = data.rotation;
            scale = data.scale;

            //info_box(ui, "The changes are applies on the Transform Component (multiplied by frame_scale for each frame). If there is no Transform Component. Nothing is happening.");

            ui.vertical(|ui|
            {
                ui.horizontal(|ui|
                {
                    ui.label("Translation: ");
                    changed = ui.add(egui::DragValue::new(&mut trans.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Rotation: ");
                    changed = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Scale: ");
                    changed = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
            });
        }

        let keys = get_keys_as_string_vec();

        let no_key = "no key";
        let mut current_key_name = no_key;

        if let Some(keyboard_key) = self.keyboard_key
        {
            current_key_name = keys[keyboard_key].as_str();
        }

        ui.horizontal(|ui|
        {
            ui.label("Keyboard key: ");
            egui::ComboBox::from_label("").selected_text(current_key_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap = Some(false);
                ui.set_min_width(60.0);

                let mut new_key = 0;
                if let Some(keyboard_key) = self.keyboard_key
                {
                    new_key = keyboard_key + 1;
                }

                let mut changed = false;

                changed = ui.selectable_value(&mut new_key, 0, "no key").changed() || changed;
                for (key_id, key) in keys.iter().enumerate()
                {
                    changed = ui.selectable_value(&mut new_key, key_id + 1, key).changed() || changed;
                }

                if changed
                {
                    if new_key == 0
                    {
                        self.keyboard_key = None;
                    }
                    else
                    {
                        self.keyboard_key = Some(new_key - 1);
                    }
                }
            });
        });

        if changed
        {
            let data = self.get_data_mut();
            let data = data.get_mut();
            data.translation = trans;
            data.rotation = rot;
            data.scale = scale;
        }
    }
}