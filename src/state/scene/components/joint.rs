use nalgebra::Matrix4;

use crate::{helper::change_tracker::ChangeTracker, component_impl_default, state::scene::node::NodeItem, component_impl_no_update_instance, input::input_manager::InputManager, component_downcast};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};


pub struct JointData
{
    pub joint_id: u32,
    pub local_trans: Matrix4<f32>,
    pub inverse_bind_trans: Matrix4<f32>,
    //pub inverse_bind_trans_calculated: Matrix4<f32>, // DEBUG?

    pub animation_weight: f32,
    pub animation_update_frame: Option<u64>,
    pub animation_trans: Option<Matrix4<f32>>
}

pub struct Joint
{
    base: ComponentBase,
    data: ChangeTracker<JointData>
}

impl Joint
{
    pub fn new(id: u64, name: &str, joint_id: u32) -> Joint
    {
        let data = JointData
        {
            joint_id,
            local_trans: Matrix4::<f32>::identity(),
            inverse_bind_trans: Matrix4::<f32>::identity(),
            //inverse_bind_trans_calculated: Matrix4::<f32>::identity(),

            animation_weight: 0.0,
            animation_update_frame: None,
            animation_trans: None,
        };

        let joint = Joint
        {
            base: ComponentBase::new(id, name.to_string(), "Joint".to_string(), "🕱".to_string()),
            data: ChangeTracker::new(data)
        };

        joint
    }

    pub fn get_data(&self) -> &JointData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<JointData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<JointData>
    {
        &mut self.data
    }

    pub fn get_animation_transform(&self) -> Matrix4<f32>
    {
        let joint_data = self.get_data();

        if let Some(animation_trans) = joint_data.animation_trans
        {
            if joint_data.animation_weight < 1.0
            {
                // animation blending - blend the animation with the initial pose if weight is smaller as 1.0
                (joint_data.local_trans * (1.0 - joint_data.animation_weight)) + (animation_trans * joint_data.animation_weight)
            }
            else
            {
                animation_trans
            }
        }
        else
        {
            joint_data.local_trans
            //Matrix4::<f32>::identity()
        }
    }
}

impl Component for Joint
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
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            if transform_component.get_data_tracker().changed()
            {
                //let transform = node.get_full_joint_transform();
                self.get_data_mut().get_mut().local_trans = transform_component.get_transform().clone();

                //self.get_data_mut().get_mut().inverse_bind_trans_calculated = transform.try_inverse().unwrap();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.label(format!("Joint Id: {}", self.get_data().joint_id));

        ui.label(format!("Inverse Bind Trans:\n{:?}", self.get_data().inverse_bind_trans));
        //ui.label(format!("Inverse Bind Trans Calculated:\n{:?}", self.get_data().inverse_bind_trans_calculated));

        ui.label(format!("Animation Trans:\n{:?}", self.get_data().animation_trans));
    }
}