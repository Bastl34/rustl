use std::sync::{Arc, RwLock};

use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update, state::scene::node::NodeItem};

use super::component::{ComponentBase, Component};

pub struct Delay
{
    base: ComponentBase,

    pub target_id: Option<u64>,
    pub delay: f32,
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
        }
    }

    pub fn new_empty(id: u64, name: &str) -> Delay
    {
        Delay
        {
            base: ComponentBase::new(id, name.to_string(), "Delay".to_string(), "⏰".to_string()),
            delay: 0.0,
            target_id: None,
        }
    }
}

impl Component for Delay
{
    component_impl_default!();
    component_impl_no_update!();
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
        };

        Some(Arc::new(RwLock::new(Box::new(delay))))
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

        ui.horizontal(|ui|
        {
            ui.set_max_width(225.0);

            ui.label("Delay: ");
            ui.add(egui::DragValue::new(&mut self.delay).speed(0.01).range(0.0..=1000.0).suffix("s"));
        });
    }
}