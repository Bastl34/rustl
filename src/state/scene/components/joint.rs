#![allow(dead_code)]

use nalgebra::{Matrix4, Quaternion, Rotation3, UnitQuaternion, Vector3};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_impl_default, component_impl_no_cleanup_node, component_impl_no_post_deserialization, component_impl_no_update_instance, helper::{change_tracker::ChangeTracker, math::approx_zero}, state::{scene::{components::{animation::AnimationLayerType, transformation::TransformationData}, node::NodeItem}, state::InputOutput}};

use super::{component::{ComponentBase, Component}, transformation::Transformation};

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct JointTransformationData
{
    pub translation: Option<Vector3<f32>>,
    pub rotation_quat: Option<nalgebra::Unit<Quaternion<f32>>>,
    pub scale: Option<Vector3<f32>>,
}

impl JointTransformationData
{
    pub fn identity() -> Self
    {
        JointTransformationData
        {
            translation: None,
            rotation_quat: None,
            scale: None,
        }
    }

    pub fn from_transformation_data(local_trans_data: &TransformationData) -> Self
    {
        let rotation_quat = if let Some(quat_vec4) = local_trans_data.rotation_quat
        {
            UnitQuaternion::new_normalize(Quaternion::new(quat_vec4.w, quat_vec4.x, quat_vec4.y, quat_vec4.z))
        }
        else
        {
            UnitQuaternion::from_euler_angles
            (
                local_trans_data.rotation.x,
                local_trans_data.rotation.y,
                local_trans_data.rotation.z
            )
        };

        JointTransformationData
        {
            translation: Some(local_trans_data.position),
            rotation_quat: Some(rotation_quat),
            scale: Some(local_trans_data.scale),
        }
    }

    pub fn to_matrix(&self) -> Matrix4<f32>
    {
        let mut trans = Matrix4::<f32>::identity();

        // Apply translation
        if let Some(translation) = &self.translation
        {
            trans = trans * nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
        }

        // Apply rotation
        if let Some(rotation) = &self.rotation_quat
        {
            let rotation: Rotation3<f32> = (*rotation).into();
            trans = trans * rotation.to_homogeneous();
        }

        // Apply scale
        if let Some(scale) = &self.scale
        {
            trans = trans * Matrix4::new_nonuniform_scaling(scale);
        }

        trans
    }

    pub fn apply_weight(&self, weight: f32) -> Self
    {
        JointTransformationData
        {
            translation: self.translation.map(|t| t * weight),
            rotation_quat: self.rotation_quat.map(|r|
            {
                let identity = nalgebra::Unit::new_normalize(Quaternion::identity());
                identity.slerp(&r, weight)
            }),
            scale: self.scale.map(|s| Vector3::new(1.0, 1.0, 1.0).lerp(&s, weight)),
        }
    }

    pub fn blend_with(&self, other: &JointTransformationData, weight: f32) -> Self
    {
        JointTransformationData
        {
            translation:
            {
                let self_trans = self.translation.unwrap_or(Vector3::zeros());
                let other_trans = other.translation.unwrap_or(Vector3::zeros());
                Some(self_trans.lerp(&other_trans, weight))
            },
            rotation_quat:
            {
                let self_rot = self.rotation_quat.unwrap_or(nalgebra::Unit::new_normalize(Quaternion::identity()));
                let other_rot = other.rotation_quat.unwrap_or(nalgebra::Unit::new_normalize(Quaternion::identity()));
                Some(self_rot.slerp(&other_rot, weight))
            },
            scale:
            {
                let self_scale = self.scale.unwrap_or(Vector3::new(1.0, 1.0, 1.0));
                let other_scale = other.scale.unwrap_or(Vector3::new(1.0, 1.0, 1.0));
                Some(self_scale.lerp(&other_scale, weight))
            },
        }
    }
}


#[derive(Serialize, Deserialize)]
pub struct JointLayeredTransformData
{
    pub layer_type: AnimationLayerType,
    pub transformation: JointTransformationData,

    pub weight: f32
}

#[derive(Serialize, Deserialize)]
pub struct JointData
{
    #[serde(skip, default)]
    pub root_joint: bool,

    #[serde(skip, default)]
    pub local_trans: JointTransformationData,

    #[serde(skip, default)]
    pub local_trans_mat: Matrix4<f32>,
    //pub full_joint_trans: Matrix4<f32>,

    #[serde(skip, default)]
    pub inverse_bind_trans: Matrix4<f32>,

    //pub inverse_bind_trans_calculated: Matrix4<f32>, // DEBUG?

    #[serde(skip, default)]
    pub animation_transforms: Vec<JointLayeredTransformData>,

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
            local_trans: JointTransformationData::identity(),
            local_trans_mat: Matrix4::<f32>::identity(),
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

        let mut total_weight: f32 = 0.0;
        let mut blended_transformation = JointTransformationData::identity();
        let mut additive_transforms: Vec<JointTransformationData> = vec![];
        let mut pose_additives: Vec<JointTransformationData> = vec![];

        for transform in &joint_data.animation_transforms
        {
            if transform.layer_type == AnimationLayerType::Blend
            {
                if total_weight == 0.0
                {
                    blended_transformation = transform.transformation.clone();
                }
                else
                {
                    let blend_factor = transform.weight / (total_weight + transform.weight);
                    blended_transformation = blended_transformation.blend_with(&transform.transformation, blend_factor);
                }
                total_weight += transform.weight;
            }
            else if transform.layer_type == AnimationLayerType::Override
            {
                blended_transformation = transform.transformation.clone();
                total_weight = transform.weight;
                additive_transforms.clear();
            }
            else if transform.layer_type == AnimationLayerType::Additive
            {
                if !approx_zero(transform.weight)
                {
                    additive_transforms.push(transform.transformation.apply_weight(transform.weight));
                }
            }
            else if transform.layer_type == AnimationLayerType::PoseAdditive
            {
                if !approx_zero(transform.weight)
                {
                    pose_additives.push(transform.transformation.apply_weight(transform.weight));
                }
            }
        }

        // Blend with local transform if total_weight < 1.0
        let final_transformation = if total_weight < 1.0 && total_weight > 0.0
        {
            let t = total_weight.clamp(0.0, 1.0);
            joint_data.local_trans.blend_with(&blended_transformation, t)
        }
        else if total_weight == 0.0
        {
            joint_data.local_trans.clone()
        }
        else
        {
            blended_transformation
        };

        // Build matrix from final transformation
        let mut trans = final_transformation.to_matrix();

        // Apply additive transforms
        for additive_transform in &additive_transforms
        {
            trans = trans * additive_transform.to_matrix();
            //trans = relative_transform.to_matrix() * trans;
        }

        // Apply pose additive transforms
        for pose_additive in &pose_additives
        {
            trans = pose_additive.to_matrix() * trans;
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

        joint_data.local_trans_mat
    }

    pub fn get_changed_local_transform(&self, node: NodeItem) -> Option<JointTransformationData>
    {
        let node = node.read().unwrap();
        let transform_component = node.find_component::<Transformation>();

        if let Some(transform_component) = transform_component
        {
            component_downcast!(transform_component, Transformation);
            if transform_component.get_data_tracker().changed()
            {
                let local_trans_data = transform_component.get_data();
                return Some(JointTransformationData::from_transformation_data(local_trans_data));
            }
        }

        None
    }

    pub fn update_local_transform(&mut self, local_trans: JointTransformationData)
    {
        let local_trans_mat = local_trans.to_matrix();
        let data = self.get_data_mut().get_mut();
        data.local_trans = local_trans;
        data.local_trans_mat = local_trans_mat;
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