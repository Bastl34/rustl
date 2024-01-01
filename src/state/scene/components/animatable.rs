use nalgebra::Matrix4;

use crate::{helper::change_tracker::ChangeTracker, component_impl_default, state::scene::node::NodeItem, component_impl_no_update_instance, input::input_manager::InputManager, component_downcast, component_downcast_mut};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};


pub struct AnimatableData
{
    pub animation_weight: f32,
    pub animation_update_frame: Option<u64>,
    pub animation_trans: Matrix4<f32>
}

pub struct Animatable
{
    base: ComponentBase,
    data: ChangeTracker<AnimatableData>
}

impl Animatable
{
    pub fn new(id: u64, name: &str) -> Animatable
    {
        let data = AnimatableData
        {
            animation_weight: 0.0,
            animation_update_frame: None,
            animation_trans: Matrix4::<f32>::identity(),
        };

        let animatable = Animatable
        {
            base: ComponentBase::new(id, name.to_string(), "Animatable".to_string(), "⛷".to_string()),
            data: ChangeTracker::new(data)
        };

        animatable
    }

    pub fn get_data(&self) -> &AnimatableData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<AnimatableData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<AnimatableData>
    {
        &mut self.data
    }

    pub fn get_animation_transform(&self) -> Matrix4<f32>
    {
        let animatable_data = self.get_data();
        animatable_data.animation_trans
    }
}

impl Component for Animatable
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
        if self.data.consume_change()
        {
            let node = node.write().unwrap();
            let transform_component = node.find_component::<Transformation>();

            if let Some(transform_component) = transform_component
            {
                component_downcast_mut!(transform_component, Transformation);
                transform_component.get_data_mut().get_mut().animation_trans = Some(self.get_data().animation_trans);
                transform_component.calc_transform();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.label(format!("Animation Trans:\n{:?}", self.get_data().animation_trans));
    }
}