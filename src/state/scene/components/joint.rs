#![allow(dead_code)]

use nalgebra::{Matrix3, Matrix4, Quaternion, Rotation3, UnitQuaternion, Vector3};
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

    pub fn from_matrix(matrix: &Matrix4<f32>) -> Self
    {
        // if shear is needed -> use SVG!

        // translation
        let translation = Vector3::new(matrix[(0, 3)], matrix[(1, 3)], matrix[(2, 3)]);

        // extract 3x3 part (rotation * scale)
        let linear = matrix.fixed_view::<3, 3>(0, 0);

        // extract scale from column lengths
        let scale = Vector3::new
        (
            linear.column(0).norm(),
            linear.column(1).norm(),
            linear.column(2).norm(),
        );

        // avoid division by zero (degenerate case)
        let safe_scale = Vector3::new
        (
            if scale.x != 0.0 { scale.x } else { 1.0 },
            if scale.y != 0.0 { scale.y } else { 1.0 },
            if scale.z != 0.0 { scale.z } else { 1.0 },
        );

        // normalize to get pure rotation
        let rotation_matrix = Matrix3::from_columns
        (&[
            linear.column(0) / safe_scale.x,
            linear.column(1) / safe_scale.y,
            linear.column(2) / safe_scale.z,
        ]);

        // convert to quaternion
        let rotation_quat = UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(rotation_matrix));

        JointTransformationData
        {
            translation: Some(translation),
            rotation_quat: Some(rotation_quat),
            scale: Some(scale),
        }
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

    // fill every property the animation does not supply with the joints bind pose value
    // glTF: a missing channel means "keep the nodes own TRS" - not identity
    // (identity would zero the translation and collapse the joint onto its parent)
    pub fn fill_missing_with_bind_pose(&self, bind_pose: &JointTransformationData) -> Self
    {
        JointTransformationData
        {
            translation: self.translation.or(bind_pose.translation),
            rotation_quat: self.rotation_quat.or(bind_pose.rotation_quat),
            scale: self.scale.or(bind_pose.scale),
        }
    }

    pub fn blend_with(&self, other: &JointTransformationData, weight: f32) -> Self
    {
        JointTransformationData
        {
            // a property that is unset on one side must not pull the other side towards zero/identity
            // -> keep the side that has a value and stay None if neither has one
            translation: match (self.translation, other.translation)
            {
                (Some(self_trans), Some(other_trans)) => Some(self_trans.lerp(&other_trans, weight)),
                (Some(self_trans), None) => Some(self_trans),
                (None, Some(other_trans)) => Some(other_trans),
                (None, None) => None,
            },
            rotation_quat: match (self.rotation_quat, other.rotation_quat)
            {
                (Some(self_rot), Some(other_rot)) => Some(self_rot.slerp(&other_rot, weight)),
                (Some(self_rot), None) => Some(self_rot),
                (None, Some(other_rot)) => Some(other_rot),
                (None, None) => None,
            },
            scale: match (self.scale, other.scale)
            {
                (Some(self_scale), Some(other_scale)) => Some(self_scale.lerp(&other_scale, weight)),
                (Some(self_scale), None) => Some(self_scale),
                (None, Some(other_scale)) => Some(other_scale),
                (None, None) => None,
            },
        }
    }

    pub fn override_with_weight(&self, other: &JointTransformationData, weight: f32) -> Self
    {
        JointTransformationData
        {
            translation:
            {
                if let Some(new_translation) = other.translation
                {
                    Some(new_translation * weight)
                }
                else
                {
                    self.translation
                }
            },
            rotation_quat:
            {
                if let Some(new_rot) = other.rotation_quat
                {
                    let identity = nalgebra::Unit::new_normalize(Quaternion::identity());
                    Some(identity.slerp(&new_rot, weight))
                }
                else
                {
                    self.rotation_quat
                }
            },
            scale:
            {
                if let Some(new_scale) = other.scale
                {
                    Some(new_scale * weight)
                }
                else
                {
                    self.scale
                }
            },
        }
    }

    pub fn additive_absolute_with_weight(&self, delta: &JointTransformationData, full_parent_transform: &Matrix4<f32>, weight: f32) -> Self
    {
        let parent_trs = JointTransformationData::from_matrix(full_parent_transform);

        // calc absolute TRS (parent * local)
        let self_translation = self.translation.unwrap_or(Vector3::zeros());
        let self_rotation = self.rotation_quat.unwrap_or(UnitQuaternion::identity());
        let self_scale = self.scale.unwrap_or(Vector3::new(1.0, 1.0, 1.0));

        let parent_rotation = parent_trs.rotation_quat.unwrap_or(UnitQuaternion::identity());
        let parent_translation = parent_trs.translation.unwrap_or(Vector3::zeros());
        let parent_scale = parent_trs.scale.unwrap_or(Vector3::new(1.0, 1.0, 1.0));

        // absolute TRS:
        let abs_translation = parent_translation + parent_rotation * (parent_scale.component_mul(&self_translation));
        let abs_rotation = parent_rotation * self_rotation;
        let abs_scale = parent_scale.component_mul(&self_scale);

        // rotation
        let blended_rotation = if let Some(delta_rot) = delta.rotation_quat
        {
            let delta_rot_weighted = UnitQuaternion::identity().slerp(&delta_rot, weight);
            Some(delta_rot_weighted * abs_rotation)
        }
        else
        {
            Some(abs_rotation)
        };

        // translation
        let blended_translation = if let Some(delta_trans) = delta.translation
        {
            Some(abs_translation + delta_trans * weight)
        }
        else
        {
            Some(abs_translation)
        };

        // scale
        let blended_scale = if let Some(delta_scale) = delta.scale
        {
            Some(abs_scale + (delta_scale - Vector3::new(1.0, 1.0, 1.0)) * weight)
        }
        else
        {
            Some(abs_scale)
        };

        // back to local space
        // (apply inverse parent to TRS)
        let inv_parent_scale = Vector3::new
        (
            1.0 / parent_scale.x,
            1.0 / parent_scale.y,
            1.0 / parent_scale.z,
        );
        let inv_parent_rotation = parent_rotation.inverse();

        let local_translation = inv_parent_rotation * ((blended_translation.unwrap() - parent_translation).component_mul(&inv_parent_scale));
        let local_rotation = inv_parent_rotation * blended_rotation.unwrap();
        let local_scale = blended_scale.unwrap().component_mul(&inv_parent_scale);

        JointTransformationData
        {
            translation: Some(local_translation),
            rotation_quat: Some(local_rotation),
            scale: Some(local_scale),
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

    pub fn get_joint_transform(&self, full_parent_joint_transform: &Matrix4<f32>) -> Matrix4<f32>
    {
        let joint_data = self.get_data();

        let mut total_weight: f32 = 0.0;
        let mut result_transformation = JointTransformationData::identity();
        let mut additive_transforms: Vec<JointTransformationData> = vec![];

        for transform in &joint_data.animation_transforms
        {
            if transform.layer_type == AnimationLayerType::Blend
            {
                if total_weight == 0.0
                {
                    result_transformation = transform.transformation.clone();
                }
                else
                {
                    let blend_factor = transform.weight / (total_weight + transform.weight);
                    result_transformation = result_transformation.blend_with(&transform.transformation, blend_factor);
                }
                total_weight += transform.weight;
            }
            else if transform.layer_type == AnimationLayerType::OverrideComponent
            {
                if total_weight == 0.0
                {
                    result_transformation = transform.transformation.clone();
                }
                else
                {
                    result_transformation = result_transformation.override_with_weight(&transform.transformation, transform.weight);
                }
            }
            else if transform.layer_type == AnimationLayerType::AdditiveComponentAbsolute
            {
                if total_weight == 0.0
                {
                    result_transformation = transform.transformation.clone();
                }
                else
                {
                    result_transformation = result_transformation.additive_absolute_with_weight(&transform.transformation, &full_parent_joint_transform, transform.weight);
                }
            }
            else if transform.layer_type == AnimationLayerType::Override
            {
                result_transformation = transform.transformation.clone();
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
        }

        // Blend with local transform if total_weight < 1.0
        let final_transformation = if total_weight < 1.0 && total_weight > 0.0
        {
            let t = total_weight.clamp(0.0, 1.0);
            joint_data.local_trans.blend_with(&result_transformation, t)
        }
        else if total_weight == 0.0
        {
            joint_data.local_trans.clone()
        }
        else
        {
            // properties without an animation channel fall back to the bind pose instead of identity
            result_transformation.fill_missing_with_bind_pose(&joint_data.local_trans)
        };

        // Build matrix from final transformation
        let mut trans = final_transformation.to_matrix();

        // Apply additive transforms
        for additive_transform in &additive_transforms
        {
            trans = trans * additive_transform.to_matrix();
            //trans = relative_transform.to_matrix() * trans;
        }

        trans
    }

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
        //ui.label(format!("Animation Transf:\n{:?}", self.get_joint_transform()));
    }
}


#[cfg(test)]
mod tests
{
    use super::*;

    // Neck joint of resourcesLocal/objects/temp/Alien.gltf - the animation only has a rotation channel there
    fn alien_neck_bind_pose() -> JointTransformationData
    {
        JointTransformationData
        {
            translation: Some(Vector3::new(0.0, 0.156, -0.081)),
            rotation_quat: Some(UnitQuaternion::identity()),
            scale: Some(Vector3::new(1.0, 1.0, 1.0)),
        }
    }

    // a channel set that only animates the rotation - translation and scale stay unset
    fn rotation_only_animation() -> JointTransformationData
    {
        JointTransformationData
        {
            translation: None,
            rotation_quat: Some(UnitQuaternion::from_euler_angles(0.3, 0.0, 0.0)),
            scale: None,
        }
    }

    #[test]
    fn unanimated_translation_keeps_the_bind_pose()
    {
        let result = rotation_only_animation().fill_missing_with_bind_pose(&alien_neck_bind_pose());
        let bind = alien_neck_bind_pose();

        assert_eq!(result.translation, bind.translation, "the joint offset must survive a rotation only animation");
        assert_eq!(result.scale, bind.scale, "the scale must survive a rotation only animation");
        assert_eq!(result.rotation_quat, rotation_only_animation().rotation_quat, "the animated rotation must win");
    }

    #[test]
    fn unanimated_translation_survives_the_matrix()
    {
        let matrix = rotation_only_animation().fill_missing_with_bind_pose(&alien_neck_bind_pose()).to_matrix();

        // without the bind pose fallback this collapses onto the parent origin -> the mesh gets squashed
        assert!((matrix[(0, 3)] - 0.0).abs() < 1.0e-6, "x was {}", matrix[(0, 3)]);
        assert!((matrix[(1, 3)] - 0.156).abs() < 1.0e-6, "y was {}", matrix[(1, 3)]);
        assert!((matrix[(2, 3)] - (-0.081)).abs() < 1.0e-6, "z was {}", matrix[(2, 3)]);
    }

    #[test]
    fn blending_does_not_pull_unanimated_properties_to_zero()
    {
        let bind = alien_neck_bind_pose();

        // half weight: the rotation blends, but translation and scale have nothing to blend towards
        let result = bind.blend_with(&rotation_only_animation(), 0.5);

        assert_eq!(result.translation, bind.translation, "a half weighted animation must not move an unanimated joint");
        assert_eq!(result.scale, bind.scale, "a half weighted animation must not scale an unanimated joint");
    }

    #[test]
    fn blending_two_unset_properties_stays_unset()
    {
        let empty = JointTransformationData::identity();
        let result = empty.blend_with(&rotation_only_animation(), 0.5);

        // must stay None so that the bind pose fallback can still fill it in later
        assert_eq!(result.translation, None);
        assert_eq!(result.scale, None);
    }
}
