use std::{collections::{HashSet, HashMap}, cell::RefCell};

use egui::mutex::RwLock;
use nalgebra::{Matrix4, Point3, Vector3};

use crate::{helper::change_tracker::ChangeTracker, component_impl_default, state::scene::node::NodeItem, component_impl_no_update_instance, input::input_manager::InputManager, component_downcast};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};


pub struct MorphTargetData
{
    pub positions: Vec<Point3<f32>>,
    pub normals: Vec<Vector3<f32>>,
    pub tangents: Vec<Vector3<f32>>,

    pub weight: RwLock<ChangeTracker<f32>>,
}

pub struct MorphTarget
{
    base: ComponentBase,
    data: ChangeTracker<MorphTargetData>
}

impl MorphTarget
{
    pub fn new(id: u64, name: &str, positions: Vec<Point3<f32>>, normals: Vec<Vector3<f32>>, tangents: Vec<Vector3<f32>>) -> MorphTarget
    {
        let data = MorphTargetData
        {
            positions,
            normals,
            tangents,

            weight: RwLock::new(ChangeTracker::new(0.0))
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

    fn update(&mut self, node: NodeItem, _input_manager: &mut InputManager, _time: u128, _frame_scale: f32, _frame: u64)
    {

    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let data = self.get_data();

        ui.horizontal(|ui|
        {
            ui.label("Weight: ");

            let mut weight = data.weight.read().get_ref().clone();

            if ui.add(egui::Slider::new(&mut weight, 0.0..=1.0).fixed_decimals(2)).changed()
            {
                *data.weight.write().get_mut() = weight;
            }
        });
    }
}