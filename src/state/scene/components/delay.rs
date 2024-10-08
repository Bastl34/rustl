use std::sync::{Arc, RwLock};

use egui::RichText;

use crate::{component_impl_default, component_impl_no_cleanup_node, helper::math::approx_zero, input::input_manager::InputManager, state::scene::node::{InstanceItemArc, NodeItem}};

use super::component::{Component, ComponentBase, ComponentItem};

pub struct Delay
{
    base: ComponentBase,

    pub target_id: Option<u64>,
    pub delay: f32,

    current_time: u128,
    pub start_time: Option<u128>,
}

impl Delay
{
    pub fn new(id: u64, name: &str, target_id: u64, delay: f32) -> Delay
    {
        Delay
        {
            base: ComponentBase::new(id, name.to_string(), "Delay".to_string(), "⏰".to_string()),
            delay,
            target_id: Some(target_id),

            current_time: 0,
            start_time: None,
        }
    }

    pub fn new_empty(id: u64, name: &str) -> Delay
    {
        Delay
        {
            base: ComponentBase::new(id, name.to_string(), "Delay".to_string(), "⏰".to_string()),
            delay: 100.0,
            target_id: None,

            current_time: 0,
            start_time: None,
        }
    }

    pub fn running(&self) -> bool
    {
        self.start_time.is_some()
    }

    /*
    pub fn delay_percentage(&self) -> f32
    {
        if let Some(start_time) = self.start_time
        {
            //let time = (self.current_time as f64 - (self.current_local_time as f64 * 1000.0 * 1000.0) * (1.0 / self.speed as f64)) as u128;
            //let local_timestamp = ((self.current_time - start_time) as f64 / 1000.0 / 1000.0) as f32;
            let delay_micros = (self.delay * 1000.0 * 1000.0) as u128;

            if start_time + delay_micros >= self.current_time
            {
                return 1.0;
            }

            let start_time_float = ((self.current_time - start_time) as f64 / 1000.0 / 1000.0) as f32;
            let to = (start_time as f64 / 1000.0 / 1000.0) as f32 + self.delay;

            return 1.0 / to * start_time_float;
        }

        0.0
    }

    */

    pub fn delay_time(&self) -> f32
    {
        if let Some(start_time) = self.start_time
        {
            let current_time = (self.current_time as f64 / 1000.0 / 1000.0) as f32;
            let start_time = (start_time as f64 / 1000.0 / 1000.0) as f32;

            return current_time - start_time;
        }

        0.0
    }

    pub fn set_current_time(&mut self, time: f32)
    {
        if let Some(start_time) = self.start_time
        {
            let time_micros = (time as f64 * 1000.0 * 1000.0) as u128 + start_time;
            let delta = time_micros - self.current_time;
            dbg!( delta as f64 / 1000.0 / 1000.0);
            self.start_time = Some(start_time + delta);
        }
    }

    pub fn start(&mut self)
    {
        if self.running()
        {
            return;
        }

        self.start_time = Some(0);
    }

    pub fn stop(&mut self)
    {
        if !self.running()
        {
            return;
        }

        self.start_time = None;
    }

    fn _update(&mut self, component: Option<ComponentItem>, _input_manager: &mut InputManager, time: u128, _frame_scale: f32, _frame: u64)
    {
        if component.is_none()
        {
            return;
        }

        let component = component.unwrap();

        self.current_time = time;

        if let Some(start_time) = self.start_time
        {
            if start_time == 0
            {
                self.start_time = Some(time);
            }
            else
            {
                let delay_micros = (self.delay * 1000.0 * 1000.0) as u128;

                if time > start_time + delay_micros
                {
                    component.write().unwrap().get_base_mut().is_enabled = true;
                    self.stop();
                }
            }
        }
    }
}

impl Component for Delay
{
    component_impl_default!();
    component_impl_no_cleanup_node!();

    fn instantiable() -> bool
    {
        true
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
        let source = self.as_any().downcast_ref::<Delay>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let delay = Delay
        {
            base: ComponentBase::duplicate(new_component_id, source.get_base()),

            delay: self.delay,
            target_id: self.target_id,

            current_time: 0,
            start_time: None,
        };

        Some(Arc::new(RwLock::new(Box::new(delay))))
    }

    fn update(&mut self, node: NodeItem, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64)
    {
        if let Some(target_id) = self.target_id
        {
            let node = node.read().unwrap();
            self._update(node.find_component_by_id(target_id), input_manager, time, frame_scale, frame);
        }
    }

    fn update_instance(&mut self, _node: NodeItem, instance: &InstanceItemArc, input_manager: &mut InputManager, time: u128, frame_scale: f32, frame: u64)
    {
        if let Some(target_id) = self.target_id
        {
            let instance = instance.read().unwrap();
            self._update(instance.find_component_by_id(target_id), input_manager, time, frame_scale, frame);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut target_id = self.target_id.unwrap_or(0);
        let mut target_name = "".to_string();

        let mut components: Vec<(u64, String)> = vec![];

        if let Some(node) = node
        {
            let node = node.read().unwrap();

            for target in &node.components
            {
                let target = target.read().unwrap();
                components.push((target.get_base().id, target.get_base().name.clone()));

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

                for target in &components
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

        let mut is_running = self.running();
        let mut is_stopped = !is_running;

        let icon_size = 20.0;

        ui.add_enabled_ui(self.target_id.is_some() && !approx_zero(self.delay), |ui|
        {
            ui.horizontal(|ui|
            {
                if ui.toggle_value(&mut is_stopped, RichText::new("⏹").size(icon_size)).on_hover_text("stop animation").clicked()
                {
                    self.stop();
                };

                if ui.toggle_value(&mut is_running, RichText::new("⏵").size(icon_size)).on_hover_text("play animation").clicked()
                {
                    self.start();
                }
            });

            ui.horizontal(|ui|
            {
                ui.label("Progress: ");

                let mut time = self.delay_time();

                if ui.add(egui::Slider::new(&mut time, 0.0..=self.delay).fixed_decimals(2).clamping(egui::SliderClamping::Edits).text("s")).changed()
                {
                    self.set_current_time(time);
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.set_max_width(225.0);

            ui.label("Delay: ");
            ui.add(egui::DragValue::new(&mut self.delay).speed(0.01).range(0.0..=1000.0).suffix("s"));
        });
    }
}