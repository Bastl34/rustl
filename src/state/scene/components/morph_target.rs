#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_post_deserialization, component_impl_no_update_instance, helper::change_tracker::ChangeTracker, state::{scene::node::NodeItem, state::InputOutput}};

use super::component::{ComponentBase, Component};

#[derive(Serialize, Deserialize)]
pub struct MorphTargetData
{
    pub target_id: u32,
    pub weight: f32,
}

#[derive(Serialize, Deserialize)]
pub struct MorphTarget
{
    base: ComponentBase,
    data: ChangeTracker<MorphTargetData>,

    extended_ui_weight_range: bool,
}

impl MorphTarget
{
    pub fn new(name: &str, target_id: u32) -> MorphTarget
    {
        let data = MorphTargetData
        {
            target_id,
            weight: 0.0
        };

        let morph_target = MorphTarget
        {
            base: ComponentBase::new(name.to_string(), "Morpth Target".to_string(), "☺".to_string()),
            data: ChangeTracker::new(data),

            extended_ui_weight_range: false,
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

#[typetag::serde]
impl Component for MorphTarget
{
    component_impl_default!();
    component_impl_no_update_instance!();
    component_impl_no_cleanup_node!();
    component_impl_no_post_deserialization!();

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

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        None
    }

    fn update(&mut self, _node: NodeItem, _io: &mut InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
    {

    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        ui.horizontal(|ui|
        {
            ui.label("Weight: ");

            let mut weight = self.get_data().weight;


            if !self.extended_ui_weight_range && ui.add(egui::Slider::new(&mut weight, 0.0..=1.0).fixed_decimals(2)).changed()
            {
                self.get_data_mut().get_mut().weight = weight;
            }
            else if self.extended_ui_weight_range && ui.add(egui::Slider::new(&mut weight, -10.0..=10.0).fixed_decimals(2)).changed()
            {
                self.get_data_mut().get_mut().weight = weight;
            }

            if ui.button("reset").clicked()
            {
                self.get_data_mut().get_mut().weight = 0.0;
            }
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.extended_ui_weight_range, "allow extended weight range (-10..10)");
        });
    }
}