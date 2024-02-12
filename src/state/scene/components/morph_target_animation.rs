use egui::Color32;
use nalgebra::{Vector3, Vector4};

use crate::{component_downcast, component_downcast_mut, component_impl_default, component_impl_no_update_instance, helper::{self, change_tracker::ChangeTracker, math::approx_equal}, input::{input_manager::InputManager, keyboard::{get_keys_as_string_vec, Key}}, state::scene::{instance::InstanceItem, node::{InstanceItemArc, NodeItem}}};

use super::{component::{Component, ComponentBase, ComponentItem}, morph_target::MorphTarget, transformation::Transformation};

const INFO_STRING: &str = "The changes are applies on the Morph Target Component.\nThey are applied for each frame.\nIf there is no Morph Target Component: Nothing is happening.";

pub struct MorphTargetAnimationData
{
    pub target_id: Option<u64>,

    pub looped: bool,
    pub periodically: bool,

    pub from: f32,
    pub to: f32,

    pub speed: f32,
    pub direction: f32,
}

pub struct MorphTargetAnimation
{
    base: ComponentBase,
    data: ChangeTracker<MorphTargetAnimationData>,

    pub keyboard_key: Option<usize>,
}

impl MorphTargetAnimation
{
    pub fn new(id: u64, name: &str, target_id: u64, from: f32, to: f32, speed: f32, looped: bool) -> MorphTargetAnimation
    {
        let data = MorphTargetAnimationData
        {
            target_id: Some(target_id),

            from,
            to,

            looped,
            periodically: true,

            speed,
            direction: 1.0
        };

        let mut animation = MorphTargetAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Morph T. Animation".to_string(), "😄".to_string()),
            data: ChangeTracker::new(data),

            keyboard_key: None,
        };

        animation.base.info = Some(INFO_STRING.to_string());

        animation
    }

    pub fn new_empty(id: u64, name: &str) -> MorphTargetAnimation
    {
        let data = MorphTargetAnimationData
        {
            target_id: None,

            from: 0.0,
            to: 1.0,

            looped: true,
            periodically: true,

            speed: 0.1,
            direction: 1.0
        };

        let mut animation = MorphTargetAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Morph T. Animation".to_string(), "😄".to_string()),
            data: ChangeTracker::new(data),

            keyboard_key: None,
        };

        animation.base.info = Some(INFO_STRING.to_string());

        animation
    }

    pub fn get_data(&self) -> &MorphTargetAnimationData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<MorphTargetAnimationData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MorphTargetAnimationData>
    {
        &mut self.data
    }
}

impl Component for MorphTargetAnimation
{
    component_impl_default!();
    component_impl_no_update_instance!();

    fn instantiable(&self) -> bool
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
        if self.get_data().target_id.is_none() || !self.base.is_enabled
        {
            return;
        }

        if let Some(keyboard_key) = self.keyboard_key
        {
            if !input_manager.keyboard.is_holding(Key::from_repr(keyboard_key).unwrap())
            {
                return;
            }
        }

        let animation_data = self.get_data_mut().get_mut();

        // find morph target component
        let node = node.read().unwrap();
        if let Some(morph_target) = node.find_component_by_id(animation_data.target_id.unwrap())
        {
            component_downcast_mut!(morph_target, MorphTarget);

            let mut weight = morph_target.get_data().weight.max(animation_data.from) + (animation_data.direction * animation_data.speed * frame_scale);

            // no loop
            if !animation_data.looped
            {
                if weight > animation_data.to
                {
                    weight = animation_data.to;
                }
                else if weight < animation_data.from
                {
                    weight = animation_data.from;
                }
            }
            // loop
            else
            {
                // not periodically
                if !animation_data.periodically
                {
                    if weight > animation_data.to
                    {
                        let delta = weight - animation_data.to;
                        if animation_data.direction > 0.0
                        {
                            // start from beginning (left)
                            weight = animation_data.from + delta;
                        }
                        else
                        {
                            // start from beginning (right)
                            weight = animation_data.to - delta;
                        }
                    }
                    else if weight < animation_data.from
                    {
                        let delta = animation_data.from - weight;

                        if animation_data.direction > 0.0
                        {
                            // start from beginning (left)
                            weight = animation_data.from + delta;
                        }
                        else
                        {
                            // start from beginning (right)
                            weight = animation_data.to - delta;
                        }
                    }
                }
                // periodically
                else
                {
                    if weight > animation_data.to
                    {
                        animation_data.direction = -1.0;

                        let delta = weight - animation_data.to;
                        weight = animation_data.to - delta;

                    }
                    else if weight < animation_data.from
                    {
                        animation_data.direction = 1.0;

                        let delta = animation_data.from - weight;
                        weight = animation_data.from + delta;
                    }
                }
            }

            weight = weight.clamp(animation_data.from, animation_data.to);

            if !approx_equal(weight, morph_target.get_data().weight)
            {
                morph_target.get_data_mut().get_mut().weight = weight;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut target_id;
        let mut target_name;
        let mut from;
        let mut to;
        let mut looped;
        let mut periodically;
        let mut speed;
        let mut direction;
        {
            let data = self.get_data();

            from = data.from;
            to = data.to;
            looped = data.looped;
            periodically = data.periodically;
            speed = data.speed;
            direction = data.direction;

            target_name = "".to_string();
            target_id = data.target_id.unwrap_or(0);
        }

        let mut morph_targets: Vec<(u64, String)> = vec![];

        if let Some(node) = node
        {
            let node = node.read().unwrap();
            let targets = node.find_components::<MorphTarget>();

            for target in targets
            {
                component_downcast!(target, MorphTarget);
                morph_targets.push((target.get_base().id, target.get_base().name.clone()));

                if target_id == target.get_base().id
                {
                    target_name = target.get_base().name.clone();
                }
            }
        }

        ui.horizontal(|ui|
        {
            ui.label("Target: ");
            egui::ComboBox::from_id_source(ui.make_persistent_id("target_id")).selected_text(target_name.clone()).show_ui(ui, |ui|
            {
                for target in &morph_targets
                {
                    changed = ui.selectable_value(&mut target_id, target.0, target.1.clone()).changed() || changed;
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("From: ");
            changed = ui.add(egui::Slider::new(&mut from, 0.0..=1.0).fixed_decimals(2)).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("To: ");
            changed = ui.add(egui::Slider::new(&mut to, 0.0..=1.0).fixed_decimals(2)).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Speed: ");
            changed = ui.add(egui::Slider::new(&mut speed, 0.0..=1.0).fixed_decimals(2)).changed() || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Direction: ");
            changed = ui.add(egui::Slider::new(&mut direction, -1.0..=1.0).fixed_decimals(0)).changed() || changed;
        });

        changed = ui.checkbox(&mut looped, "Loop").changed() || changed;
        changed = ui.checkbox(&mut periodically, "Periodically").changed() || changed;

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
            data.from = from;
            data.to = to;
            data.speed = speed;
            data.direction = direction;
            data.looped = looped;
            data.periodically = periodically;

            if target_id > 0
            {
                data.target_id = Some(target_id);
            }
            else
            {
                data.target_id = None
            }
        }
    }
}