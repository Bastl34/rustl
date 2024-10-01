#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use crate::{component_downcast, component_downcast_mut, component_impl_default, component_impl_no_cleanup_node, component_impl_no_update_instance, helper::math::approx_equal, input::{input_manager::InputManager, keyboard::{get_keys_as_string_vec, Key}}, state::scene::node::NodeItem};
use crate::helper::easing::{Easing, easing, get_easing_as_string_vec};

use super::{component::{Component, ComponentBase}, morph_target::MorphTarget};

const INFO_STRING: &str = "The changes are applies on the Morph Target Component.\nThey are applied for each frame.\nIf there is no Morph Target Component: Nothing is happening.";


pub struct MorphTargetAnimation
{
    base: ComponentBase,

    pub target_id: Option<u64>,

    pub looped: bool,
    pub ping_pong: bool,

    pub easing: Easing,

    pub from: f32,
    pub to: f32,

    pub speed: f32,
    pub direction: f32,

    weight: f32,

    pub keyboard_key: Option<usize>,
}

impl MorphTargetAnimation
{
    pub fn new(id: u64, name: &str, target_id: u64, from: f32, to: f32, speed: f32, looped: bool) -> MorphTargetAnimation
    {
        let mut animation = MorphTargetAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Morph T. Animation".to_string(), "☺".to_string()),

            target_id: Some(target_id),

            easing: Easing::None,

            from,
            to,

            looped,
            ping_pong: true,

            speed,
            direction: 1.0,

            weight: 0.0,

            keyboard_key: None,
        };

        animation.base.info = Some(INFO_STRING.to_string());

        animation
    }

    pub fn new_empty(id: u64, name: &str) -> MorphTargetAnimation
    {
        let mut animation = MorphTargetAnimation
        {
            base: ComponentBase::new(id, name.to_string(), "Morph T. Animation".to_string(), "☺".to_string()),

            target_id: None,

            from: 0.0,
            to: 1.0,

            easing: Easing::None,

            looped: true,
            ping_pong: true,

            speed: 0.1,
            direction: 1.0,

            weight: 0.0,

            keyboard_key: None,
        };

        animation.base.info = Some(INFO_STRING.to_string());

        animation
    }

    pub fn reset(&mut self)
    {
        self.weight = 0.0;
    }
}

impl Component for MorphTargetAnimation
{
    component_impl_default!();
    component_impl_no_update_instance!();
    component_impl_no_cleanup_node!();

    fn instantiable() -> bool
    {
        false
    }

    fn duplicatable(&self) -> bool
    {
        true
    }

    fn set_enabled(&mut self, state: bool)
    {
        if self.base.is_enabled != state
        {
            self.base.is_enabled = state;
        }
    }

    fn duplicate(&self, new_component_id: u64) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        let source = self.as_any().downcast_ref::<MorphTargetAnimation>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let mut animation = MorphTargetAnimation
        {
            base: ComponentBase::duplicate(new_component_id, source.get_base()),

            target_id: source.target_id.clone(),

            easing: source.easing,

            from: source.from,
            to: source.to,

            looped: source.looped,
            ping_pong: source.ping_pong,

            speed: source.speed,
            direction: source.direction,

            weight: source.weight,

            keyboard_key: source.keyboard_key.clone(),
        };

        animation.base.info = Some(INFO_STRING.to_string());

        Some(Arc::new(RwLock::new(Box::new(animation))))
    }

    fn update(&mut self, node: NodeItem, input_manager: &mut InputManager, _time: u128, frame_scale: f32, _frame: u64)
    {
        if self.target_id.is_none() || !self.base.is_enabled
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

        // find morph target component
        let node = node.read().unwrap();
        if let Some(morph_target) = node.find_component_by_id(self.target_id.unwrap())
        {
            component_downcast_mut!(morph_target, MorphTarget);

            //let mut weight = morph_target.get_data().weight.max(self.from) + (self.direction * self.speed * frame_scale);
            let mut weight = self.weight.max(self.from) + (self.direction * self.speed * frame_scale);

            // no loop
            if !self.looped
            {
                if weight > self.to
                {
                    weight = self.to;
                }
                else if weight < self.from
                {
                    weight = self.from;
                }
            }
            // loop
            else
            {
                // not ping pong
                if !self.ping_pong
                {
                    if weight > self.to
                    {
                        let delta = weight - self.to;
                        if self.direction > 0.0
                        {
                            // start from beginning (left)
                            weight = self.from + delta;
                        }
                        else
                        {
                            // start from beginning (right)
                            weight = self.to - delta;
                        }
                    }
                    else if weight < self.from
                    {
                        let delta = self.from - weight;

                        if self.direction > 0.0
                        {
                            // start from beginning (left)
                            weight = self.from + delta;
                        }
                        else
                        {
                            // start from beginning (right)
                            weight = self.to - delta;
                        }
                    }
                }
                // ping pong
                else
                {
                    if weight > self.to
                    {
                        self.direction = -1.0;

                        let delta = weight - self.to;
                        weight = self.to - delta;

                    }
                    else if weight < self.from
                    {
                        self.direction = 1.0;

                        let delta = self.from - weight;
                        weight = self.from + delta;
                    }
                }
            }


            weight = weight.clamp(self.from, self.to);
            self.weight = weight;

            // easing
            let delta_t = self.to - self.from;
            weight = easing(self.easing, weight / delta_t) * delta_t;

            if !approx_equal(weight, morph_target.get_data().weight)
            {
                morph_target.get_data_mut().get_mut().weight = weight;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut target_id = self.target_id.unwrap_or(0);
        let mut target_name = "".to_string();

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
            egui::ComboBox::from_id_salt(ui.make_persistent_id("target_id")).width(160.0).selected_text(target_name.clone()).show_ui(ui, |ui|
            {
                let mut changed = false;

                for target in &morph_targets
                {
                    changed = ui.selectable_value(&mut target_id, target.0, target.1.clone()).changed() || changed;
                }

                if changed
                {
                    if target_id > 0
                    {
                        self.target_id = Some(target_id);
                    }
                    else
                    {
                        self.target_id = None
                    }
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Easing: ");

            let easings = get_easing_as_string_vec();
            let current_easing_name = easings[self.easing as usize].as_str();
            egui::ComboBox::from_id_salt(ui.make_persistent_id("easing_id")).selected_text(current_easing_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(30.0);

                let mut current_easing_id = self.easing as usize;

                let mut changed = false;
                for (easing_id, easing) in easings.iter().enumerate()
                {
                    changed = ui.selectable_value(&mut current_easing_id, easing_id, easing).changed() || changed;
                }

                if changed
                {
                    self.easing = Easing::from_repr(current_easing_id).unwrap()
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("From: ");
            ui.add(egui::Slider::new(&mut self.from, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("To: ");
            ui.add(egui::Slider::new(&mut self.to, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Speed: ");
            ui.add(egui::Slider::new(&mut self.speed, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Progress: ");
            ui.add(egui::Slider::new(&mut self.weight, 0.0..=1.0).fixed_decimals(2))
        });

        ui.horizontal(|ui|
        {
            ui.label("Direction: ");
            ui.add(egui::Slider::new(&mut self.direction, -1.0..=1.0).fixed_decimals(0));
        });

        ui.checkbox(&mut self.looped, "Loop");
        ui.checkbox(&mut self.ping_pong, "Ping Pong");

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
            egui::ComboBox::from_id_salt(ui.make_persistent_id("keyboad_id")).selected_text(current_key_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
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
    }
}