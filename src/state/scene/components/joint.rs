use std::collections::{HashSet, HashMap};

use nalgebra::{Matrix4, Quaternion, Rotation3, UnitQuaternion, Vector3, Vector4};

use crate::{component_downcast, component_impl_default, component_impl_no_update_instance, helper::{change_tracker::ChangeTracker, math::approx_equal}, input::input_manager::InputManager, state::scene::node::NodeItem};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};


pub struct JointData
{
    pub root_joint: bool,
    pub local_trans: Matrix4<f32>,
    pub full_joint_trans: Matrix4<f32>,
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
    //pub fn new(id: u64, name: &str, joint_id: u32) -> Joint
    pub fn new(id: u64, name: &str) -> Joint
    {
        let data = JointData
        {
            root_joint: false,
            full_joint_trans: Matrix4::<f32>::identity(),
            local_trans: Matrix4::<f32>::identity(),
            inverse_bind_trans: Matrix4::<f32>::identity(),
            //inverse_bind_trans_calculated: Matrix4::<f32>::identity(),

            animation_weight: 0.0,
            animation_update_frame: None,

            animation_trans: None
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

    pub fn get_inverse_bind_transform(&self) -> Matrix4<f32>
    {
        self.get_data().inverse_bind_trans
        //self.get_data().inverse_bind_trans_calculated
    }

    pub fn get_joint_transform(&self) -> Matrix4<f32>
    {
        let joint_data = self.get_data();

        if let Some(animation_trans) = self.get_animation_transform()
        {
            if joint_data.animation_weight < 1.0
            {
                let animation_weight = joint_data.animation_weight.clamp(0.0, 1.0);
                joint_data.local_trans * (1.0 - animation_weight) + animation_trans * animation_weight
            }
            else if joint_data.animation_weight > 1.0
            {
                animation_trans * (1.0 / joint_data.animation_weight)
            }
            else
            {
                //joint_data.local_trans * animation_trans // sometimes this is correct (For some models)
                animation_trans
            }
        }
        else
        {
            joint_data.local_trans
        }
    }

    pub fn get_animation_transform(&self) -> Option<Matrix4<f32>>
    {
        self.get_data().animation_trans
    }

    pub fn get_local_transform(&self) -> Matrix4<f32>
    {
        let joint_data = self.get_data();

        joint_data.local_trans
    }

    pub fn get_changed_local_transform(&self, node: NodeItem) -> Option<Matrix4<f32>>
    {
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            if transform_component.get_data_tracker().changed()
            {
                let local_trans = transform_component.get_transform().clone();
                return Some(local_trans);
            }
        }

        None
    }

    pub fn update_local_transform(&mut self, local_trans: Matrix4<f32>)
    {
        self.get_data_mut().get_mut().local_trans = local_trans;
    }

    fn get_full_inverse_bind_transform(node: NodeItem) -> Matrix4<f32>
    {
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            let local_trans = transform_component.get_transform().clone();

            let mut inverse_bindpose_matrix = local_trans.try_inverse().unwrap();

            if let Some(parent) = &node.parent
            {
                //if parent.read().unwrap().find_component::<Joint>().is_some()
                if !parent.read().unwrap().root_node
                {
                    let parent_inverse_bindpose_matrix = Self::get_full_inverse_bind_transform(parent.clone());
                    inverse_bindpose_matrix = parent_inverse_bindpose_matrix * inverse_bindpose_matrix;
                }
            }

            return inverse_bindpose_matrix;
        }

        Matrix4::identity()
    }
}

impl Component for Joint
{
    component_impl_default!();
    component_impl_no_update_instance!();

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

    fn update(&mut self, node: NodeItem, _input_manager: &mut InputManager, _time: u128, _frame_scale: f32, _frame: u64)
    {
        let local_trans = self.get_changed_local_transform(node.clone());

        if let Some(local_trans) = local_trans
        {
            self.update_local_transform(local_trans);

        }

        //let inverse_bind_transform = Self::get_full_inverse_bind_transform(node.clone());
        //self.get_data_mut().get_mut().inverse_bind_trans_calculated = inverse_bind_transform;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        ui.label(format!("Root Joint: {}", self.get_data().root_joint));

        let bind_transform = self.get_data().inverse_bind_trans.try_inverse().unwrap();

        ui.label(format!("Inverse Bind Trans:\n{:?}", self.get_data().inverse_bind_trans));
        ui.label(format!("Bind Trans:\n{:?}", bind_transform));
        //ui.label(format!("Inverse Bind Trans Calculated:\n{:?}", self.get_data().inverse_bind_trans_calculated));
        ui.label(format!("Animation Transf:\n{:?}", self.get_data().animation_trans));
    }
}