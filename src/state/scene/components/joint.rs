#![allow(dead_code)]

use nalgebra::{Matrix4, Quaternion, Rotation3, Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_impl_default, component_impl_no_cleanup_node, component_impl_no_post_deserialization, component_impl_no_update_instance, helper::{change_tracker::ChangeTracker, math::{approx_zero, interpolate_matrices}}, state::{scene::{components::animation::AnimationLayerType, node::NodeItem}, state::InputOutput}};

use super::{component::{ComponentBase, Component}, transformation::Transformation};

#[derive(Serialize, Deserialize)]
pub struct JointTransformData
{
    pub layer_type: AnimationLayerType,
    pub translation: Option<Vector3<f32>>,
    pub rotation_quat: Option<nalgebra::Unit<Quaternion<f32>>>,
    pub scale: Option<Vector3<f32>>,

    pub weight: f32
}

impl JointTransformData
{
    pub fn get_full_animation_transform(&self) -> Matrix4<f32>
    {
        let mut trans = Matrix4::<f32>::identity();

        // translation
        if let Some(translation) = &self.translation
        {
            trans = trans * nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
        }

        // rotation
        if let Some(data_rotation_quat) = &self.rotation_quat
        {
            let rotation: Rotation3<f32> = (*data_rotation_quat).into();
            let rotation = rotation.to_homogeneous();

            trans = trans * rotation;
        }

        // scale
        if let Some(animation_scale) = &self.scale
        {
            trans = trans * Matrix4::new_nonuniform_scaling(&animation_scale);
        }

        trans
    }

    pub fn get_weighted_transform(&self, weight: f32) -> Matrix4<f32>
    {
        let mut trans = Matrix4::<f32>::identity();

        // translation
        if let Some(translation) = &self.translation
        {
            let translation = translation * weight;
            trans = trans * nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
        }

        // rotation
        if let Some(rotation_quat) = &self.rotation_quat
        {
            let rotation_quat = rotation_quat.powf(weight);
            let rotation: Rotation3<f32> = rotation_quat.into();
            let rotation = rotation.to_homogeneous();

            trans = trans * rotation;
        }

        // scale
        if let Some(animation_scale) = &self.scale
        {
            let scale = Vector3::new(1.0, 1.0, 1.0).lerp(&animation_scale, weight);
            trans = trans * Matrix4::new_nonuniform_scaling(&scale);
        }

        trans
    }


}

#[derive(Serialize, Deserialize)]
pub struct JointData
{
    #[serde(skip, default)]
    pub root_joint: bool,

    #[serde(skip, default)]
    pub local_trans: Matrix4<f32>,
    //pub full_joint_trans: Matrix4<f32>,

    #[serde(skip, default)]
    pub inverse_bind_trans: Matrix4<f32>,
    //pub inverse_bind_trans_calculated: Matrix4<f32>, // DEBUG?

    #[serde(skip, default)]
    pub animation_transforms: Vec<JointTransformData>,

    //#[serde(skip, default)]
    //pub animation_weight: f32,

    #[serde(skip, default)]
    pub animation_update_frame: Option<u64>,

    //#[serde(skip, default)]
    //pub animation_trans: Option<Matrix4<f32>>
}

#[derive(Serialize, Deserialize)]
pub struct Joint
{
    base: ComponentBase,

    data: ChangeTracker<JointData>
}

impl Joint
{
    //pub fn new(id: u64, name: &str, joint_id: u32) -> Joint
    pub fn new(name: &str) -> Joint
    {
        let data = JointData
        {
            root_joint: false,
            //full_joint_trans: Matrix4::<f32>::identity(),
            local_trans: Matrix4::<f32>::identity(),
            inverse_bind_trans: Matrix4::<f32>::identity(),
            //inverse_bind_trans_calculated: Matrix4::<f32>::identity(),

            //animation_weight: 0.0,
            animation_update_frame: None,

            //animation_trans: None

            animation_transforms: vec![]
        };

        let joint = Joint
        {
            base: ComponentBase::new(name.to_string(), "Joint".to_string(), "🕱".to_string()),
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

        let mut weight: f32 = 0.0;
        let mut trans = Matrix4::<f32>::identity();
        let mut relative_transforms = vec![];

        for transform in &joint_data.animation_transforms
        {
            if transform.layer_type == AnimationLayerType::Blend
            {
                let transform_mat = transform.get_full_animation_transform();
                if weight == 0.0
                {
                    trans = transform_mat * transform.weight;
                }
                else
                {
                    let total_weight = weight + transform.weight;
                    if total_weight > 0.0
                    {
                        let t = transform.weight / total_weight;
                        trans = interpolate_matrices(&trans, &transform_mat, t);
                    }
                }
                weight += transform.weight;
            }
            else if transform.layer_type == AnimationLayerType::Override
            {
                trans = transform.get_full_animation_transform();
                weight = transform.weight;
                relative_transforms.clear();
            }
            else if transform.layer_type == AnimationLayerType::Relative
            {
                if !approx_zero(transform.weight)
                {
                    relative_transforms.push(transform.get_weighted_transform(transform.weight));
                }
            }
        }

        // Blend with local transform if weight < 1.0
        if weight < 1.0 && weight > 0.0
        {
            let t = weight.clamp(0.0, 1.0);
            trans = interpolate_matrices(&joint_data.local_trans, &trans, t);
        }
        else if weight == 0.0
        {
            trans = joint_data.local_trans;
        }

        // Apply relative transforms (only once!)
        for relative_transform in &relative_transforms
        {
            trans = trans * relative_transform;
        }

        trans
    }

    /*
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
                //joint_data.local_trans * animation_trans // sometimes this is correct (For some models) - Alien
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
     */

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

    fn get_full_transform_inverse_bind_transform(node: NodeItem) -> Matrix4<f32>
    {
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);

            let local_trans = transform_component.get_transform().clone();

            let mut inverse_bind_pose_matrix = local_trans.try_inverse().unwrap();

            if let Some(parent) = node.parent.as_ref()
            {
                //if parent.read().unwrap().find_component::<Joint>().is_some()
                if !parent.read().unwrap().root_node
                {
                    let parent_inverse_bind_pose_matrix = Self::get_full_transform_inverse_bind_transform(parent.clone());
                    inverse_bind_pose_matrix = parent_inverse_bind_pose_matrix * inverse_bind_pose_matrix;
                }
            }

            return inverse_bind_pose_matrix;
        }

        Matrix4::identity()
    }
}

#[typetag::serde]
impl Component for Joint
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

    fn update(&mut self, node: NodeItem, _io: &mut InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
    {
        let local_trans = self.get_changed_local_transform(node.clone());

        if let Some(local_trans) = local_trans
        {
            self.update_local_transform(local_trans);

        }

        //let inverse_bind_transform = Self::get_full_transform_inverse_bind_transform(node.clone());
        //self.get_data_mut().get_mut().inverse_bind_trans_calculated = inverse_bind_transform;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        ui.label(format!("Root Joint: {}", self.get_data().root_joint));

        let bind_transform = self.get_data().inverse_bind_trans.try_inverse().unwrap();

        ui.label(format!("Inverse Bind Trans:\n{:?}", self.get_data().inverse_bind_trans));
        ui.label(format!("Bind Trans:\n{:?}", bind_transform));
        //ui.label(format!("Inverse Bind Trans Calculated:\n{:?}", self.get_data().inverse_bind_trans_calculated));
        ui.label(format!("Animation Transf:\n{:?}", self.get_joint_transform()));
    }
}