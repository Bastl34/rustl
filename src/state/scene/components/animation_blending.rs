#![allow(dead_code)]

use crate::{component_downcast, component_downcast_mut, component_impl_default, component_impl_no_update_instance, helper::math::{approx_equal, approx_zero}, input::input_manager::InputManager, state::scene::node::NodeItem};

use super::{animation::Animation, component::{Component, ComponentBase}};

const INFO_STRING: &str = "The changes are applies on the Animation Component.\nIf there is no Animation Component: Nothing is happening.";

pub struct AnimationBlending
{
    base: ComponentBase,

    pub from: Option<u64>,
    pub to: Option<u64>,

    pub speed: f32,
}

impl AnimationBlending
{
    pub fn new(id: u64, name: &str, from: Option<u64>, to: Option<u64>, speed: f32) -> AnimationBlending
    {
        let mut animation_blending = AnimationBlending
        {
            base: ComponentBase::new(id, name.to_string(), "Animation Blending".to_string(), "◑".to_string()),

            from,
            to,
            speed
        };

        animation_blending.base.info = Some(INFO_STRING.to_string());

        animation_blending
    }

    pub fn new_empty(id: u64, name: &str) -> AnimationBlending
    {
        let mut animation_blending = AnimationBlending
        {
            base: ComponentBase::new(id, name.to_string(), "Animation Blending".to_string(), "◑".to_string()),

            from: None,
            to: None,
            speed: 0.0,
        };

        animation_blending.base.info = Some(INFO_STRING.to_string());

        animation_blending
    }
}

impl Component for AnimationBlending
{
    component_impl_default!();
    component_impl_no_update_instance!();

    fn instantiable(&self) -> bool
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

    fn update(&mut self, node: NodeItem, _input_manager: &mut InputManager, _time: u128, frame_scale: f32, _frame: u64)
    {
        //if (self.get_data().from.is_none() && self.get_data().to.is_none()) || !self.base.is_enabled
        if !self.base.is_enabled
        {
            return;
        }

        if approx_zero(self.speed)
        {
            return;
        }

        if self.from.is_some() && self.to.is_some() && self.to == self.from
        {
            return;
        }

        // animation to
        let node = node.read().unwrap();
        if let Some(to) = self.to
        {
            if let Some(animation_to) = node.find_component_by_id(to)
            {
                component_downcast_mut!(animation_to, Animation);

                if animation_to.running() && approx_equal(animation_to.weight, 1.0)
                {
                    return;
                }

                // start with no weight on target animation
                let mut from_weight = animation_to.weight;
                if !animation_to.running()
                {
                    from_weight = 0.0;
                }

                animation_to.weight = (from_weight + (self.speed * frame_scale)).min(1.0);
                animation_to.start();
            }
        }
        else
        {
            // fade out
            let all_animations = node.find_components::<Animation>();
            for animation in all_animations
            {
                component_downcast_mut!(animation, Animation);
                if animation.running() && (self.from.is_none() || self.from.unwrap() != animation.get_base().id)
                {
                    animation.weight = (animation.weight - (self.speed * frame_scale)).max(0.0);

                    if approx_zero(animation.weight)
                    {
                        animation.stop_without_reset();
                    }
                }
            }
        }

        // animation from
        if let Some(from) = self.from
        {
            if let Some(animation_from) = node.find_component_by_id(from)
            {
                component_downcast_mut!(animation_from, Animation);
                //animation_from.weight = (1.0 - to_weight).max(0.0);
                animation_from.weight = (animation_from.weight - (self.speed * frame_scale)).max(0.0);

                if approx_zero(animation_from.weight)
                {
                    animation_from.stop_without_reset();
                }
            }
        }
        else
        {
            let all_animations = node.find_components::<Animation>();
            for animation in all_animations
            {
                component_downcast_mut!(animation, Animation);
                if animation.running() && (self.to.is_none() || self.to.unwrap() != animation.get_base().id)
                {
                    animation.weight = (animation.weight - (self.speed * frame_scale)).max(0.0);

                    if approx_zero(animation.weight)
                    {
                        animation.stop_without_reset();
                    }
                }
            }
        }

    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut from;
        let mut from_name;

        let mut to: u64;
        let mut to_name;

        let mut speed;

        {
            from = self.from.unwrap_or(0);
            from_name = "".to_string();

            to = self.to.unwrap_or(0);
            to_name = "".to_string();

            speed = self.speed;
        }

        let mut animations: Vec<(u64, String)> = vec![];

        if let Some(node) = node
        {
            let node = node.read().unwrap();
            let animation_components = node.find_components::<Animation>();

            for animation in animation_components
            {
                component_downcast!(animation, Animation);
                animations.push((animation.get_base().id, animation.get_base().name.clone()));

                if from == animation.get_base().id
                {
                    from_name = animation.get_base().name.clone();
                }
                if to == animation.get_base().id
                {
                    to_name = animation.get_base().name.clone();
                }
            }
        }

        ui.horizontal(|ui|
        {
            ui.label("From: ");
            egui::ComboBox::from_id_source(ui.make_persistent_id("from")).selected_text(from_name.clone()).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut from, 0, "").changed() || changed;
                for animation in &animations
                {
                    changed = ui.selectable_value(&mut from, animation.0, animation.1.clone()).changed() || changed;
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("To: ");
            egui::ComboBox::from_id_source(ui.make_persistent_id("to")).selected_text(to_name.clone()).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut to, 0, "").changed() || changed;
                for animation in &animations
                {
                    changed = ui.selectable_value(&mut to, animation.0, animation.1.clone()).changed() || changed;
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Speed: ");
            changed = ui.add(egui::Slider::new(&mut speed, 0.0..=1.0).fixed_decimals(3)).changed() || changed;
        });


        if changed
        {
            if from > 0
            {
                self.from = Some(from);
            }
            else
            {
                self.from = None
            }

            if to > 0
            {
                self.to = Some(to);
            }
            else
            {
                self.to = None
            }

            self.speed = speed;
        }
    }
}