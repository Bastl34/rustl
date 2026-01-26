#![allow(dead_code)]

use std::{sync::{Arc, RwLock}};

use egui::RichText;
use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, console_error, helper::{math::approx_zero, option_or_id::OptionOrId}, state::{scene::node::{InstanceItemArc, NodeItem}, state::InputOutput}};
use crate::state::scene::exporter::serialization_helper;

use super::component::{Component, ComponentBase, ComponentItem};

#[derive(Serialize, Deserialize)]
pub struct Delay
{
    base: ComponentBase,

    #[serde(serialize_with = "serialization_helper::serialize_component", deserialize_with = "serialization_helper::deserialize_component")]
    pub target: OptionOrId<ComponentItem>,

    pub delay: f32,
    pub state: bool,
    pub repeat: bool,

    #[serde(skip, default)]
    current_time: u128,

    #[serde(skip, default)]
    pub start_time: Option<u128>,
}

impl Delay
{
    pub fn new(name: &str, target: ComponentItem, delay: f32) -> Delay
    {
        Delay
        {
            base: ComponentBase::new(name.to_string(), "Delay".to_string(), "⏰".to_string()),
            delay,
            target: OptionOrId::Some(target),
            state: true,
            repeat: false,

            current_time: 0,
            start_time: None,
        }
    }

    pub fn new_empty(name: &str) -> Delay
    {
        Delay
        {
            base: ComponentBase::new(name.to_string(), "Delay".to_string(), "⏰".to_string()),
            delay: 0.0,
            target: OptionOrId::None,
            state: true,
            repeat: false,

            current_time: 0,
            start_time: None,
        }
    }

    pub fn running(&self) -> bool
    {
        self.start_time.is_some()
    }

    pub fn delay_time(&self) -> f32
    {
        if self.current_time == 0
        {
            return 0.0;
        }

        if let Some(start_time) = self.start_time
        {
            if start_time == 0
            {
                return 0.0;
            }

            let diff = ((self.current_time - start_time) as f64 / 1000.0 / 1000.0) as f32;
            return diff;
        }

        0.0
    }

    pub fn set_current_time(&mut self, time: f32)
    {
        if let Some(start_time) = self.start_time
        {
            let time_micros = (time as f64 * 1000.0 * 1000.0) as u128 + start_time;
            let delta = time_micros - self.current_time;

            self.start_time = Some(start_time - delta);
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

    fn _update(&mut self, component: Option<ComponentItem>, _io: &mut InputOutput, time: u128, _frame_scale: f32, _frame: u64)
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
                    component.write().unwrap().get_base_mut().is_enabled = self.state;
                    self.stop();

                    if self.repeat
                    {
                        self.start();
                    }
                }
            }
        }
    }
}

#[typetag::serde]
impl Component for Delay
{
    component_impl_default!();
    component_impl_no_cleanup_node!();

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        if self.target.is_ref()
        {
            // resolve component
            let component = context.components.iter().find(|c| c.read().unwrap().get_base().uuid == self.target.id().unwrap());
            if let Some(component) = component
            {
                self.target = OptionOrId::Some(component.clone());
            }
            else
            {
                self.target = OptionOrId::None;
                console_error!("Delay: target with id {} not found", self.target.id().unwrap());
            }
        }
        else
        {
            self.target = OptionOrId::None;
            console_error!("Delay: no target found");
        }
    }

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

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        let source = self.as_any().downcast_ref::<Delay>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let delay = Delay
        {
            base: ComponentBase::duplicate(source.get_base()),

            delay: self.delay,
            target: self.target.clone(),
            state: self.state,
            repeat: self.repeat,

            current_time: 0,
            start_time: None,
        };

        Some(Arc::new(RwLock::new(Box::new(delay))))
    }

    fn update(&mut self, _node: NodeItem, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64)
    {
        self._update(self.target.as_ref().cloned(), io, time, frame_scale, frame);
    }

    fn update_instance(&mut self, _node: Option<NodeItem>, _instance: &InstanceItemArc, io: &mut InputOutput, time: u128, frame_scale: f32, frame: u64)
    {
        self._update(self.target.as_ref().cloned(), io, time, frame_scale, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut target_name = "".to_string();

        let mut target_id = if let Some(target) = self.target.as_ref()
        {
            target.read().unwrap().id()
        }
        else
        {
            0
        };

        let mut components: Vec<(u32, ComponentItem)> = vec![];

        if let Some(node) = node
        {
            let node = node.read().unwrap();

            for component_arc in &node.components
            {
                let component = component_arc.read().unwrap();
                components.push((component.get_base().id, component_arc.clone()));

                if target_id == component.get_base().id
                {
                    target_name = component.get_base().name.clone();
                }
            }
        }

        ui.horizontal(|ui|
        {
            ui.label("Target: ");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("target_id")).width(160.0).selected_text(target_name.clone()).show_ui(ui, |ui|
            {
                let mut changed = false;

                for (component_id, component) in &components
                {
                    changed = ui.selectable_value(&mut target_id, *component_id, component.read().unwrap().get_base().name.clone()).changed() || changed;
                }

                if changed
                {
                    if target_id > 0
                    {
                        let component = components.iter().find(|(id, _)| *id == target_id);

                        self.target = OptionOrId::Some(component.unwrap().1.clone());
                    }
                    else
                    {
                        self.target = OptionOrId::None
                    }
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("State:");
            ui.selectable_value(& mut self.state, true, "Enable");
            ui.selectable_value(& mut self.state, false, "Disable");
        });

        ui.checkbox(&mut self.repeat, "Repeat");

        let mut is_running = self.running();
        let mut is_stopped = !is_running;

        let icon_size = 20.0;

        ui.add_enabled_ui(self.target.is_some() && !approx_zero(self.delay), |ui|
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

            ui.add_enabled_ui(self.running(), |ui|
            {
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
        });

        ui.horizontal(|ui|
        {
            ui.set_max_width(225.0);

            ui.label("Delay: ");
            ui.add(egui::DragValue::new(&mut self.delay).speed(0.01).range(0.0..=1000.0).suffix("s"));
        });
    }
}