use egui::Color32;
use nalgebra::{Vector3, Vector4};

use crate::{helper::{change_tracker::ChangeTracker, self}, component_impl_default, state::{scene::{node::{NodeItem, InstanceItemArc}, instance::InstanceItem}}, component_downcast, component_downcast_mut, input::{input_manager::InputManager, keyboard::{Key, get_keys_as_string_vec}}};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};

const INFO_STRING: &str = "The changes are applies on the Transform Component.\nThey are multiplied by frame_scale for each frame.\nIf there is no Transform Component: Nothing is happening.";

pub struct TransformationAnimationData
{
    pub translation: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub rotation_quat: Option<Vector4<f32>>,
    pub scale: Vector3<f32>,
}

pub struct TransformationAnimation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationAnimationData>,

    pub keyboard_key: Option<usize>,

    ui_lock_translation: bool,
    ui_lock_rotation: bool,
    ui_lock_rotation_quat: bool,
    ui_lock_scale: bool,
}

impl TransformationAnimation
{
    pub fn new(id: u64, name: &str, translation: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> TransformationAnimation
    {
        let data = TransformationAnimationData
        {
            translation,
            rotation,
            rotation_quat: None,
            scale
        };

        let mut transform_animation = TransformationAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Transform. Animation".to_string(), "🚤".to_string()),
            data: ChangeTracker::new(data),

            keyboard_key: None,

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
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
            rotation_quat: None,
            scale: Vector3::<f32>::zeros()
        };

        let mut transform_animation = TransformationAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Transform. Animation".to_string(), "🚤".to_string()),
            data: ChangeTracker::new(data),

            keyboard_key: None,

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
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

    fn _update(&mut self, transform_component: Option<ComponentItem>, input_manager: &mut InputManager, _time: u128, frame_scale: f32, _frame: u64)
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

            if let Some(rotation_quat) = data.rotation_quat
            {
                if !helper::math::approx_zero_vec4(&rotation_quat)
                {
                    transform_component.apply_rotation_quaternion(rotation_quat);
                }
            }
        }
    }
}

impl Component for TransformationAnimation
{
    component_impl_default!();

    fn instantiable() -> bool
    {
        true
    }

    fn duplicatable(&self) -> bool
    {
        false
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

    fn update(&mut self, node: NodeItem, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64)
    {
        let node = node.write().unwrap();
        self._update(node.find_component::<Transformation>(), input_manager, time, frame_scale, frame);
    }

    fn update_instance(&mut self, _node: NodeItem, instance: &InstanceItemArc, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64)
    {
        let instance = instance.read().unwrap();
        self._update(instance.find_component::<Transformation>(), input_manager, time, frame_scale, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut trans;
        let mut rot;
        let mut rot_quat;
        let mut scale;
        {
            let data = self.get_data();

            trans = data.translation;
            rot = data.rotation;
            rot_quat = data.rotation_quat;
            scale = data.scale;

            //info_box(ui, "The changes are applies on the Transform Component (multiplied by frame_scale for each frame). If there is no Transform Component. Nothing is happening.");

            ui.vertical(|ui|
            {
                ui.horizontal(|ui|
                {
                    ui.label("Translation: ");
                    let changed_x = ui.add(egui::DragValue::new(&mut trans.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut trans.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut trans.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_translation, "🔒").on_hover_text("same position value for all coordinates");

                    if self.ui_lock_translation  && changed_x { trans.y = trans.x; trans.z = trans.x; }
                    if self.ui_lock_translation  && changed_y { trans.x = trans.y; trans.z = trans.y; }
                    if self.ui_lock_translation  && changed_z { trans.x = trans.z; trans.y = trans.z; }

                    changed = changed_x || changed_y || changed_z || changed;
                });

                ui.horizontal(|ui|
                {
                    ui.label("Rotation\n(Euler): ");
                    let changed_x = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_rotation, "🔒").on_hover_text("same rotation value for all coordinates");

                    if self.ui_lock_rotation  && changed_x { rot.y = rot.x; rot.z = rot.x; }
                    if self.ui_lock_rotation  && changed_y { rot.x = rot.y; rot.z = rot.y; }
                    if self.ui_lock_rotation  && changed_z { rot.x = rot.z; rot.y = rot.z; }

                    changed = changed_x || changed_y || changed_z || changed;
                });

                if let Some(rot_quat) = rot_quat.as_mut()
                {
                    ui.horizontal(|ui|
                    {
                        ui.label("Rotation\n(Quaternion): ");
                        let changed_x = ui.add(egui::DragValue::new(&mut rot_quat.x).speed(0.1).prefix("x: ")).changed();
                        let changed_y = ui.add(egui::DragValue::new(&mut rot_quat.y).speed(0.1).prefix("y: ")).changed();
                        let changed_z = ui.add(egui::DragValue::new(&mut rot_quat.z).speed(0.1).prefix("z: ")).changed();
                        let changed_w = ui.add(egui::DragValue::new(&mut rot_quat.w).speed(0.1).prefix("w: ")).changed();
                        ui.toggle_value(&mut self.ui_lock_rotation_quat, "🔒").on_hover_text("same rotation value for all coordinates (x, y, z)");

                        if self.ui_lock_rotation_quat  && changed_x { rot_quat.y = rot_quat.x; rot_quat.z = rot_quat.x; }
                        if self.ui_lock_rotation_quat  && changed_y { rot_quat.x = rot_quat.y; rot_quat.z = rot_quat.y; }
                        if self.ui_lock_rotation_quat  && changed_z { rot_quat.x = rot_quat.z; rot_quat.y = rot_quat.z; }

                        changed = changed_x || changed_y || changed_z || changed_w || changed;
                    });
                }

                ui.horizontal(|ui|
                {
                    ui.label("Scale: ");
                    let changed_x = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_scale, "🔒").on_hover_text("same scaling value for all coordinates");

                    if self.ui_lock_scale  && changed_x { scale.y = scale.x; scale.z = scale.x; }
                    if self.ui_lock_scale  && changed_y { scale.x = scale.y; scale.z = scale.y; }
                    if self.ui_lock_scale  && changed_z { scale.x = scale.z; scale.y = scale.z; }

                    changed = changed_x || changed_y || changed_z || changed;
                });

                if rot_quat.is_none()
                {
                    if ui.button("add Quaternion Rotation").clicked()
                    {
                        rot_quat = Some(Vector4::<f32>::new(0.0, 0.0, 0.0, 0.0));
                        changed = true;
                    }
                }
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
            egui::ComboBox::from_id_source(ui.make_persistent_id("keyboad_id")).selected_text(current_key_name).show_ui(ui, |ui|
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
            data.rotation_quat = rot_quat;
            data.scale = scale;
        }
    }
}