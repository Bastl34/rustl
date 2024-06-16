use std::any::Any;

use crate::{helper::change_tracker::ChangeTracker, component_impl_default, state::scene::node::{NodeItem, InstanceItemArc}, component_impl_no_update};

use super::component::{ComponentBase, Component};

pub struct AlphaData
{
    pub alpha_inheritance: bool,

    pub alpha: f32,
}

pub struct Alpha
{
    base: ComponentBase,
    data: ChangeTracker<AlphaData>
}

impl Alpha
{
    pub fn new(id: u64, name: &str, alpha: f32) -> Alpha
    {
        let data = AlphaData
        {
            alpha_inheritance: true,
            alpha
        };

        let alpha = Alpha
        {
            base: ComponentBase::new(id, name.to_string(), "Alpha".to_string(), "🌖".to_string()),
            data: ChangeTracker::new(data)
        };

        alpha
    }

    pub fn get_data(&self) -> &AlphaData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<AlphaData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<AlphaData>
    {
        &mut self.data
    }

    pub fn has_alpha_inheritance(&self) -> bool
    {
        self.data.get_ref().alpha_inheritance
    }

    pub fn get_alpha(&self) -> f32
    {
        self.data.get_ref().alpha
    }
}

impl Component for Alpha
{
    component_impl_default!();
    component_impl_no_update!();

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

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut alpha;
        let mut alpha_inheritance;

        {
            let data = self.get_data();

            alpha = data.alpha;
            alpha_inheritance = data.alpha_inheritance;

            changed = ui.add(egui::Slider::new(&mut alpha, 0.0..=1.0).text("alpha")).changed() || changed;
            changed = ui.checkbox(&mut alpha_inheritance, "alpha inheritance").changed() || changed;
        }

        if changed
        {
            let data = self.get_data_mut();
            data.get_mut().alpha = alpha;
            data.get_mut().alpha_inheritance = alpha_inheritance;
        }
    }
}