use std::collections::{HashSet, HashMap};

use nalgebra::{Matrix4, Quaternion, Rotation3, UnitQuaternion, Vector3, Vector4};

use crate::{helper::change_tracker::ChangeTracker, component_impl_default, state::scene::node::NodeItem, component_impl_no_update_instance, input::input_manager::InputManager, component_downcast};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};


pub struct JointData
{
    pub root_joint: bool,
    //pub joint_id: u32,
    pub skin_ids: HashMap<u32, u32>,
    pub local_trans: Matrix4<f32>,
    pub full_joint_trans: Matrix4<f32>,
    pub inverse_bind_trans: Matrix4<f32>,
    //pub inverse_bind_trans_calculated: Matrix4<f32>, // DEBUG?

    pub animation_weight: f32,
    pub animation_update_frame: Option<u64>,

    pub animation_trans: Option<Matrix4<f32>>
    //pub animation_position: Option<Vector3<f32>>,
    //pub animation_rotation_quat: Option<Vector4<f32>>,
    //pub animation_scale: Option<Vector3<f32>>,
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
            //joint_id,
            skin_ids: HashMap::new(),
            full_joint_trans: Matrix4::<f32>::identity(),
            local_trans: Matrix4::<f32>::identity(),
            inverse_bind_trans: Matrix4::<f32>::identity(),
            //inverse_bind_trans_calculated: Matrix4::<f32>::identity(),

            animation_weight: 0.0,
            animation_update_frame: None,

            animation_trans: None,
            //animation_position: None,
            //animation_rotation_quat: None,
            //animation_scale: None,
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

    pub fn get_joint_transform(&self) -> Matrix4<f32>
    {
        let joint_data = self.get_data();

        if let Some(animation_trans) = self.get_animation_transform()
        {
            if joint_data.animation_weight < 1.0
            {
                // animation blending - blend the animation with the initial pose if weight is smaller as 1.0
                //(joint_data.local_trans * (1.0 - joint_data.animation_weight)) + (animation_trans * joint_data.animation_weight)
                //joint_data.full_joint_trans * (joint_data.local_trans * (1.0 - joint_data.animation_weight)) + (animation_trans * joint_data.animation_weight)

                //(animation_trans * joint_data.animation_weight) * (joint_data.local_trans * (1.0 - joint_data.animation_weight))

                //(joint_data.local_trans * (1.0 - joint_data.animation_weight)) + (animation_trans * joint_data.animation_weight)

                let animation_weight = joint_data.animation_weight.clamp(0.0, 1.0);
                joint_data.local_trans * (1.0 - animation_weight) + animation_trans * animation_weight
            }
            else
            {
                //joint_data.local_trans * animation_trans
                //joint_data.full_joint_trans * animation_trans
                animation_trans
            }
        }
        else
        {
            joint_data.local_trans
            //joint_data.full_joint_trans * joint_data.local_trans
            //Matrix4::<f32>::identity()
        }
    }

    pub fn get_animation_transform(&self) -> Option<Matrix4<f32>>
    {
        self.get_data().animation_trans
        /*
        let data = self.get_data();

        if data.animation_position.is_none() && data.animation_rotation_quat.is_none() && data.animation_scale.is_none()
        {
            return None;
        }

        let mut trans = Matrix4::<f32>::identity();

        // translation
        if let Some(animation_position) = &data.animation_position
        {
            trans = trans * nalgebra::Isometry3::translation(animation_position.x, animation_position.y, animation_position.z).to_homogeneous();
        }

        // rotation
        if let Some(data_rotation_quat) = &data.animation_rotation_quat
        {
            let quaternion = UnitQuaternion::new_normalize
            (
                Quaternion::new
                (
                    data_rotation_quat.w,
                    data_rotation_quat.x,
                    data_rotation_quat.y,
                    data_rotation_quat.z,
                )
            );

            let rotation: Rotation3<f32> = quaternion.into();
            let rotation = rotation.to_homogeneous();

            trans = trans * rotation;
        }

        // scale
        if let Some(animation_scale) = &data.animation_scale
        {
            trans = trans * Matrix4::new_nonuniform_scaling(&animation_scale);
        }

        Some(trans)
         */
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
        let local_trans = self.get_changed_local_transform(node);

        if let Some(local_trans) = local_trans
        {
            self.update_local_transform(local_trans);

        }
        /*
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            if transform_component.get_data_tracker().changed()
            {
                let local_trans = transform_component.get_transform().clone();
                //let local_trans_inverse = local_trans.try_inverse().unwrap();

                if self.get_data().root_joint
                //{
                //    self.get_data_mut().get_mut().full_joint_trans = node.get_full_joint_transform();
                //}

                self.get_data_mut().get_mut().local_trans = local_trans;
                //self.get_data_mut().get_mut().full_joint_trans = node.get_full_joint_transform();

                //self.get_data_mut().get_mut().inverse_bind_trans_calculated = transform.try_inverse().unwrap();
            }
        }
         */
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        ui.label(format!("Root Joint: {}", self.get_data().root_joint));
        ui.label(format!("Skin Ids: {:?}", self.get_data().skin_ids));

        ui.label(format!("Inverse Bind Trans:\n{:?}", self.get_data().inverse_bind_trans));
        //ui.label(format!("Inverse Bind Trans Calculated:\n{:?}", self.get_data().inverse_bind_trans_calculated));
        ui.label(format!("Animation Transf:\n{:?}", self.get_data().animation_trans));

        /*
        if let Some(animation_position) = self.get_data().animation_position
        {
            ui.label(format!("Animation position: [{:.3}, {:.3}, {:.3}]", animation_position.x, animation_position.z, animation_position.z));
        }

        if let Some(animation_rotation_quat) = self.get_data().animation_rotation_quat
        {
            ui.label(format!("Animation rotation (quat): [{:.3}, {:.3}, {:.3}, {:.3}]", animation_rotation_quat.x, animation_rotation_quat.z, animation_rotation_quat.z,animation_rotation_quat.w));
        }

        if let Some(animation_scale) = self.get_data().animation_scale
        {
            ui.label(format!("Animation scale: [{:.3}, {:.3}, {:.3}]", animation_scale.x, animation_scale.z, animation_scale.z));
        }
         */
    }
}