
use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update_instance, helper::change_tracker::ChangeTracker, input::input_manager::InputManager, state::scene::node::NodeItem};

use super::component::{ComponentBase, Component};


pub struct MorphTargetData
{
    pub target_id: u32,
    pub weight: f32,
}

pub struct MorphTarget
{
    base: ComponentBase,
    data: ChangeTracker<MorphTargetData>
}

impl MorphTarget
{
    pub fn new(id: u64, name: &str, target_id: u32) -> MorphTarget
    {
        let data = MorphTargetData
        {
            target_id,
            weight: 0.0
        };

        let morph_target = MorphTarget
        {
            base: ComponentBase::new(id, name.to_string(), "Morpth Target".to_string(), "☺".to_string()),
            data: ChangeTracker::new(data)
        };

        morph_target
    }

    pub fn get_data(&self) -> &MorphTargetData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<MorphTargetData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<MorphTargetData>
    {
        &mut self.data
    }


}

impl Component for MorphTarget
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

    fn duplicate(&self, _new_component_id: u64) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        None
    }

    fn update(&mut self, node: NodeItem, _input_manager: &mut InputManager, _time: u128, _frame_scale: f32, _frame: u64)
    {

    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        ui.horizontal(|ui|
        {
            ui.label("Weight: ");

            let mut weight = self.get_data().weight;

            if ui.add(egui::Slider::new(&mut weight, 0.0..=1.0).fixed_decimals(2)).changed()
            {
                self.get_data_mut().get_mut().weight = weight;
            }

            if ui.button("reset").clicked()
            {
                self.get_data_mut().get_mut().weight = 0.0;
            }
        });
    }
}