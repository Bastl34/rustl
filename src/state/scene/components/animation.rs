#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use std::collections::HashMap;

use egui::{Color32, RichText};
use nalgebra::{Matrix4, Vector3, Vector4, Quaternion, UnitQuaternion, Rotation3};

use crate::{component_downcast_mut, component_impl_default, component_impl_no_update_instance, helper::{easing::Easing, easing::easing, easing::get_easing_as_string_vec, math::{approx_zero, cubic_spline_interpolate_vec, cubic_spline_interpolate_vec3, cubic_spline_interpolate_vec4, interpolate_vec, interpolate_vec3}}, input::input_manager::InputManager, state::scene::{components::joint::Joint, node::NodeItem, scene::Scene}};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation, morph_target::MorphTarget};

#[derive(PartialEq, Debug, Clone)]
pub enum Interpolation
{
    Linear,
    Step,
    CubicSpline
}

#[derive(Clone)]
pub struct Channel
{
    pub interpolation: Interpolation,
    pub timestamps: Vec<f32>,

    pub transform_translation: Vec<Vector3<f32>>,
    pub transform_rotation: Vec<Vector4<f32>>,
    pub transform_scale: Vec<Vector3<f32>>,
    pub transform_morph: Vec<Vec<f32>>,

    pub target: NodeItem
}

impl Channel
{
    pub fn new(target: NodeItem) -> Channel
    {
        Channel
        {
            interpolation: Interpolation::Linear,
            timestamps: vec![],

            transform_translation: vec![],
            transform_rotation: vec![],
            transform_scale: vec![],
            transform_morph: vec![],

            target
        }
    }
}

#[derive(Clone)]
struct TargetMapItem
{
    pub component: ComponentItem,
    pub position: Option<Vector3<f32>>,
    pub rotation_quat: Option<nalgebra::Unit<Quaternion<f32>>>,
    pub scale: Option<Vector3<f32>>,
    pub skip_joint: bool
}

pub struct Animation
{
    base: ComponentBase,

    pub looped: bool,
    pub reverse: bool,

    pub easing: Easing,

    pub from: f32,
    pub to: f32,
    pub duration: f32, // based on animation data (to prevent that the animation is longer as the duration)

    pub start_time: Option<u128>,
    pub pause_time: Option<u128>,

    pub weight: f32,
    pub speed: f32,

    pub channels: Vec<Channel>,

    pub joint_filter: Vec<(NodeItem, bool)>, // only apply parts of the animation for specific nodes

    current_time: u128,
    current_local_time: f32,

    ui_joint_include_option: bool
}

impl Animation
{
    pub fn new(id: u64, name: &str) -> Animation
    {
        Animation
        {
            base: ComponentBase::new(id, name.to_string(), "Animation".to_string(), "🎞".to_string()),

            looped: true,
            reverse: false,

            easing: Easing::None,

            from: 0.0,
            to: 0.0,
            duration: 0.0,

            start_time: None,
            pause_time: None,

            weight: 1.0,
            speed: 1.0,

            channels: vec![],

            joint_filter: vec![],

            current_time: 0,
            current_local_time: 0.0,

            ui_joint_include_option: true
        }
    }

    pub fn running(&self) -> bool
    {
        self.start_time.is_some()
    }

    pub fn paused(&self) -> bool
    {
        self.pause_time.is_some()
    }

    pub fn percentage(&self) -> f32
    {
        if !self.running()
        {
            return 0.0;
        }

        1.0 / self.to * self.current_local_time
    }

    pub fn animation_time(&self) -> f32
    {
        self.current_local_time % self.to
    }

    pub fn start(&mut self)
    {
        if self.running()
        {
            return;
        }

        self.start_time = Some(0);
        self.pause_time = None;
    }

    pub fn resume(&mut self)
    {
        let time = (self.current_time as f64 - (self.current_local_time as f64 * 1000.0 * 1000.0) * (1.0 / self.speed as f64)) as u128;

        self.start_time = Some(time);
        self.pause_time = None;
    }

    pub fn stop(&mut self)
    {
        if !self.running()
        {
            return;
        }

        self.start_time = None;
        self.reset();
    }

    pub fn stop_without_reset(&mut self)
    {
        if !self.running()
        {
            return;
        }

        self.start_time = None;
    }

    pub fn pause(&mut self)
    {
        if self.start_time.is_none() && self.pause_time.is_none()
        {
            return;
        }

        if self.pause_time.is_none()
        {
            self.pause_time = Some(self.current_time);
            self.start_time = None;
        }
    }

    pub fn set_current_time(&mut self, time: f32)
    {
        self.current_local_time = time % self.to;
        self.resume();
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.speed = speed;
    }

    pub fn is_over(&self) -> bool
    {
        if self.current_local_time >= self.to && !self.looped
        {
            return true;
        }

        false
    }

    pub fn check_is_over(&self, time: u128) -> bool
    {
        if self.is_over()
        {
            return true;
        }

        let t = self.get_local_time(time);

        if !self.looped && t > self.to
        {
            return true;
        }

        return false;
    }

    pub fn reset(&mut self)
    {
        for channel in &self.channels
        {
            let target = channel.target.write().unwrap();

            if let Some(joint) = target.find_component::<Joint>()
            {
                component_downcast_mut!(joint, Joint);

                joint.get_data_mut().get_mut().animation_trans = None;

                joint.get_data_mut().get_mut().animation_update_frame = None;
                joint.get_data_mut().get_mut().animation_weight = 0.0;
            }

            if let Some(transformation) = target.find_component::<Transformation>()
            {
                component_downcast_mut!(transformation, Transformation);

                transformation.get_data_mut().get_mut().animation_position = None;
                transformation.get_data_mut().get_mut().animation_rotation_quat = None;
                transformation.get_data_mut().get_mut().animation_scale = None;

                transformation.get_data_mut().get_mut().animation_update_frame = None;
                transformation.get_data_mut().get_mut().animation_weight = 0.0;
                transformation.calc_transform();
            }
        }

        self.start_time = None;
        self.pause_time = None;
        self.current_time = 0;
        self.current_local_time = 0.0;
    }

    pub fn get_local_time(&self, time: u128) -> f32
    {
        let start_time = self.start_time.unwrap();

        let local_timestamp = ((time - start_time) as f64 / 1000.0 / 1000.0) as f32;
        let current_local_time = local_timestamp * self.speed;

        current_local_time
    }
}

fn apply_transformation_to_target(target_map: &mut HashMap<u64, TargetMapItem>, target_id: u64, transform: &(Option<Vector3<f32>>, Option<nalgebra::Unit<Quaternion<f32>>>, Option<Vector3<f32>>))
{
    // transformation
    if let Some(animation_position) = transform.0
    {
        let target_item = target_map.get_mut(&target_id).unwrap();

        if target_item.position.is_none()
        {
            target_item.position = Some(animation_position);
        }
        else
        {
            target_item.position = Some(target_item.position.unwrap() + animation_position);
        }
    }

    // rotation
    if let Some(animation_rotation_quat) = transform.1
    {
        let target_item = target_map.get_mut(&target_id).unwrap();

        if target_item.rotation_quat.is_none()
        {
            target_item.rotation_quat = Some(animation_rotation_quat);
        }
        else
        {
            target_item.rotation_quat = Some(target_item.rotation_quat.unwrap() * animation_rotation_quat);
        }
    }

    // scale
    if let Some(animation_scale) = transform.2
    {
        let target_item = target_map.get_mut(&target_id).unwrap();

        if target_item.scale.is_none()
        {
            target_item.scale = Some(animation_scale);
        }
        else
        {
            let x = target_item.scale.unwrap().x * animation_scale.x;
            let y = target_item.scale.unwrap().y * animation_scale.y;
            let z = target_item.scale.unwrap().z * animation_scale.z;
            target_item.scale = Some(Vector3::<f32>::new(x, y, z));
        }
    }
}

fn get_animation_transform(transform: &TargetMapItem) -> Matrix4<f32>
{
    let mut trans = Matrix4::<f32>::identity();

    // translation
    if let Some(animation_position) = &transform.position
    {
        trans = trans * nalgebra::Isometry3::translation(animation_position.x, animation_position.y, animation_position.z).to_homogeneous();
    }

    // rotation
    if let Some(data_rotation_quat) = &transform.rotation_quat
    {
        let rotation: Rotation3<f32> = (*data_rotation_quat).into();
        let rotation = rotation.to_homogeneous();

        trans = trans * rotation;
    }

    // scale
    if let Some(animation_scale) = &transform.scale
    {
        trans = trans * Matrix4::new_nonuniform_scaling(&animation_scale);
    }

    trans
}

impl Component for Animation
{
    component_impl_default!();
    component_impl_no_update_instance!();

    fn instantiable() -> bool
    {
        false
    }

    fn duplicatable(&self) -> bool
    {
        true
    }

    fn set_enabled(&mut self, state: bool)
    {
        if self.base.is_enabled != state
        {
            self.base.is_enabled = state;
        }
    }

    fn cleanup_node(&mut self, node: NodeItem) -> bool
    {
        let channels_amount = self.channels.len();

        self.channels.retain(|channel|
        {
            channel.target.read().unwrap().id != node.read().unwrap().id
        });

        channels_amount != self.channels.len()
    }

    fn duplicate(&self, new_component_id: u64) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        let source = self.as_any().downcast_ref::<Animation>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let animation = Animation
        {
            base: ComponentBase::duplicate(new_component_id, source.get_base()),

            looped: self.looped,
            reverse: self.reverse,

            easing: self.easing,

            from: self.from,
            to: self.to,
            duration: self.duration,

            start_time: self.start_time,
            pause_time: self.pause_time,

            weight: self.weight,
            speed: self.speed,

            channels: self.channels.clone(),

            joint_filter: self.joint_filter.clone(),

            current_time: 0,
            current_local_time: 0.0,

            ui_joint_include_option: self.ui_joint_include_option
        };

        Some(Arc::new(RwLock::new(Box::new(animation))))
    }

    fn update(&mut self, _node: NodeItem, _input_manager: &mut InputManager, time: u128, _frame_scale: f32, frame: u64)
    {
        self.current_time = time;

        if !self.running()
        {
            return;
        }

        if let Some(start_time) = self.start_time
        {
            if start_time == 0
            {
                self.start_time = Some(time);
            }
        }

        // do not update if animation is already over
        if self.start_time.is_none()
        {
            return;
        }

        self.current_local_time = self.get_local_time(time);
        let mut t = self.current_local_time;

        if !self.looped && t > self.to
        {
            self.stop_without_reset();
            return;
        }

        let delta = self.to - self.from;

        // animation
        if !approx_zero(delta)
        {
            t = (t % delta) + self.from;

            //if self.reverse { t = self.to - t; }
            if self.reverse { t = self.to + self.from - t; }

            // easing
            t = easing(self.easing, t / delta) * delta;
        }
        // pose
        else
        {
            t = 0.0;
        }

        let mut target_map: HashMap<u64, TargetMapItem> = HashMap::new();

        // ********** reset joints (if needed) **********
        for channel in &self.channels
        {
            let target = channel.target.write().unwrap();
            let joint = target.find_component::<Joint>();
            let transformation = target.find_component::<Transformation>();

            if let Some(joint) = joint
            {
                let joint_clone = joint.clone();

                component_downcast_mut!(joint, Joint);

                let data = joint.get_data_mut().get_mut();

                if data.animation_update_frame == None || data.animation_update_frame.unwrap() != frame
                {
                    joint.get_data_mut().get_mut().animation_trans = Some(Matrix4::<f32>::identity());

                    joint.get_data_mut().get_mut().animation_update_frame = Some(frame);
                    joint.get_data_mut().get_mut().animation_weight = 0.0;
                }

                target_map.insert(joint.id(), TargetMapItem{ component: joint_clone, position: None, rotation_quat: None, scale: None, skip_joint: false });
            }
            else if let Some(transformation) = transformation
            {
                let transformation_clone = transformation.clone();

                component_downcast_mut!(transformation, Transformation);

                let data = transformation.get_data_mut().get_mut();

                if data.animation_update_frame == None || data.animation_update_frame.unwrap() != frame
                {
                    transformation.get_data_mut().get_mut().animation_position = None;
                    transformation.get_data_mut().get_mut().animation_rotation_quat = None;
                    transformation.get_data_mut().get_mut().animation_scale = None;

                    transformation.get_data_mut().get_mut().animation_update_frame = Some(frame);
                    transformation.get_data_mut().get_mut().animation_weight = 0.0;
                }

                target_map.insert(transformation.id(), TargetMapItem{ component: transformation_clone, position: None, rotation_quat: None, scale: None, skip_joint: false });
            }
        }

        // ********** calculate animation matrix **********
        for channel in &self.channels
        {
            let mut joint_included_found = false;
            let mut joint_excluded_found = false;

            for (joint, include) in &self.joint_filter
            {
                let node = joint;

                if channel.target.read().unwrap().has_parent_or_is_equal(node.clone())
                {
                    if *include
                    {
                        joint_included_found = true;
                    }
                    else
                    {
                        joint_excluded_found = true;
                    }

                }
            }

            let mut skip_joint = false;
            if joint_excluded_found
            {
                skip_joint = true;
            }

            if joint_included_found
            {
                skip_joint = false;
            }

            let joint;
            {
                let target = channel.target.read().unwrap();
                joint = target.find_component::<Joint>();
            }

            let transformation;
            {
                let target = channel.target.read().unwrap();

                transformation = target.find_component::<Transformation>();
            }

            if joint.is_none() && transformation.is_none()
            {
                // NOT SUPPORTED
                dbg!("not supported for now");
                continue;
            }

            let mut target_id = 0;
            if let Some(joint) = &joint
            {
                target_id = joint.read().unwrap().id();
            } else if let Some(transformation) = transformation
            {
                target_id = transformation.read().unwrap().id();
            }


            // ********** only one item per channel **********
            if channel.timestamps.len() <= 1
            {
                let mut transform = (None, None, None);
                if channel.transform_translation.len() > 0
                {
                    let t = &channel.transform_translation[0];

                    transform.0 = Some(t.clone());
                }
                else if channel.transform_rotation.len() > 0
                {
                    let r = &channel.transform_rotation[0];
                    let quaternion = UnitQuaternion::new_normalize(Quaternion::new(r.w, r.x, r.y, r.z));
                    transform.1 = Some(quaternion);
                }
                else if channel.transform_scale.len() > 0
                {
                    let s = &channel.transform_scale[0];
                    transform.2 = Some(s.clone());
                }
                else if channel.transform_morph.len() > 0
                {
                    let weights = &channel.transform_morph[0];

                    let target = channel.target.read().unwrap();
                    let morph_targets = target.find_components::<MorphTarget>();

                    for morph_target in morph_targets
                    {
                        component_downcast_mut!(morph_target, MorphTarget);

                        for (target_id, weight) in weights.iter().enumerate()
                        {
                            if morph_target.get_data().target_id == target_id as u32
                            {
                                let morph_target_data = morph_target.get_data_mut().get_mut();
                                morph_target_data.weight = *weight * self.weight;
                            }
                        }
                    }
                }

                apply_transformation_to_target(&mut target_map, target_id, &transform);

                // skip joint flag
                if transform.0.is_some() || transform.1.is_some() || transform.2.is_some()
                {
                    let target_item = target_map.get_mut(&target_id).unwrap();
                    target_item.skip_joint = skip_joint;
                }
            }
            // ********** some items per channel **********
            else
            {
                let min = channel.timestamps[0];
                let len = channel.timestamps.len();
                let max = channel.timestamps[len - 1];

                let mut t = t;
                if t < min { t = min; }
                if t > max { t = max; }

                let mut t0 = 0;
                let mut t1 = 0;
                for (i, &start) in channel.timestamps[..len - 1].iter().enumerate()
                {
                    //TODO: store last value (for optimization?!)
                    let next = channel.timestamps[i + 1];

                    if t >= start && t <= next
                    {
                        t0 = i;
                        t1 = i + 1;
                        break;
                    }
                }

                let prev_time = channel.timestamps[t0];
                let next_time = channel.timestamps[t1];
                let factor = (t - prev_time) / (next_time - prev_time);

                // ********** translation **********
                if channel.transform_translation.len() > 0
                {
                    let translation = match channel.interpolation
                    {
                        Interpolation::Linear =>
                        {
                            let from = &channel.transform_translation[t0];
                            let to = &channel.transform_translation[t1];

                            interpolate_vec3(&from, &to, factor)
                        },
                        Interpolation::Step =>
                        {
                            channel.transform_translation[t0].clone()
                        },
                        Interpolation::CubicSpline =>
                        {
                            let delta_time = next_time - prev_time;

                            let l = t0 * 3;

                            let prev_input_tangent = &channel.transform_translation[l];
                            let prev_keyframe_value = &channel.transform_translation[l+1];
                            let prev_output_tangent = &channel.transform_translation[l+2];

                            let r = t1 * 3;

                            let next_input_tangent = &channel.transform_translation[r];
                            let next_keyframe_value = &channel.transform_translation[r+1];
                            let next_output_tangent = &channel.transform_translation[r+2];

                            let res = cubic_spline_interpolate_vec3
                            (
                                factor,
                                delta_time,
                                prev_input_tangent,
                                prev_keyframe_value,
                                prev_output_tangent,
                                next_input_tangent,
                                next_keyframe_value,
                                next_output_tangent,
                            );

                            res
                        },
                    };

                    apply_transformation_to_target(&mut target_map, target_id, &(Some(translation), None, None));
                }
                // ********** rotation **********
                else if channel.transform_rotation.len() > 0
                {
                    let rotation = match channel.interpolation
                    {
                        Interpolation::Linear =>
                        {
                            let from = &channel.transform_rotation[t0];
                            let to = &channel.transform_rotation[t1];

                            let quaternion0 = UnitQuaternion::new_normalize(Quaternion::new(from.w, from.x, from.y, from.z));
                            let quaternion1 = UnitQuaternion::new_normalize(Quaternion::new(to.w, to.x, to.y, to.z));

                            quaternion0.slerp(&quaternion1, factor)
                        },
                        Interpolation::Step =>
                        {
                            let from = &channel.transform_rotation[t0];

                            UnitQuaternion::new_normalize(Quaternion::new(from.w, from.x, from.y, from.z))
                        },
                        Interpolation::CubicSpline =>
                        {
                            let delta_time = next_time - prev_time;

                            let l = t0 * 3;

                            let prev_input_tangent = &channel.transform_rotation[l];
                            let prev_keyframe_value = &channel.transform_rotation[l+1];
                            let prev_output_tangent = &channel.transform_rotation[l+2];

                            let r = t1 * 3;

                            let next_input_tangent = &channel.transform_rotation[r];
                            let next_keyframe_value = &channel.transform_rotation[r+1];
                            let next_output_tangent = &channel.transform_rotation[r+2];

                            let res = cubic_spline_interpolate_vec4
                            (
                                factor,
                                delta_time,
                                prev_input_tangent,
                                prev_keyframe_value,
                                prev_output_tangent,
                                next_input_tangent,
                                next_keyframe_value,
                                next_output_tangent,
                            );

                            UnitQuaternion::new_normalize(Quaternion::new(res.w, res.x, res.y, res.z))
                        },
                    };

                    apply_transformation_to_target(&mut target_map, target_id, &(None, Some(rotation), None));
                }
                // ********** scale **********
                else if channel.transform_scale.len() > 0
                {
                    let scale = match channel.interpolation
                    {
                        Interpolation::Linear =>
                        {
                            let from = &channel.transform_scale[t0];
                            let to = &channel.transform_scale[t1];

                            interpolate_vec3(&from, &to, factor)
                        },
                        Interpolation::Step =>
                        {
                            channel.transform_scale[t0].clone()
                        },
                        Interpolation::CubicSpline =>
                        {
                            let delta_time = next_time - prev_time;

                            let l = t0 * 3;

                            let prev_input_tangent = &channel.transform_scale[l];
                            let prev_keyframe_value = &channel.transform_scale[l+1];
                            let prev_output_tangent = &channel.transform_scale[l+2];

                            let r = t1 * 3;

                            let next_input_tangent = &channel.transform_scale[r];
                            let next_keyframe_value = &channel.transform_scale[r+1];
                            let next_output_tangent = &channel.transform_scale[r+2];

                            let res = cubic_spline_interpolate_vec3
                            (
                                factor,
                                delta_time,
                                prev_input_tangent,
                                prev_keyframe_value,
                                prev_output_tangent,
                                next_input_tangent,
                                next_keyframe_value,
                                next_output_tangent,
                            );

                            res
                        },
                    };

                    apply_transformation_to_target(&mut target_map, target_id, &(None, None, Some(scale)));
                }
                // ********** morph targets **********
                else if channel.transform_morph.len() > 0
                {
                    let weights = match channel.interpolation
                    {
                        Interpolation::Linear =>
                        {
                            let from = &channel.transform_morph[t0];
                            let to = &channel.transform_morph[t1];

                            interpolate_vec(&from, &to, factor)
                        },
                        Interpolation::Step =>
                        {
                            channel.transform_morph[t0].clone()
                        },
                        Interpolation::CubicSpline =>
                        {
                            let delta_time = next_time - prev_time;

                            let l = t0 * 3;

                            let prev_input_tangent = &channel.transform_morph[l];
                            let prev_keyframe_value = &channel.transform_morph[l+1];
                            let prev_output_tangent = &channel.transform_morph[l+2];

                            let r = t1 * 3;

                            let next_input_tangent = &channel.transform_morph[r];
                            let next_keyframe_value = &channel.transform_morph[r+1];
                            let next_output_tangent = &channel.transform_morph[r+2];

                            cubic_spline_interpolate_vec
                            (
                                factor,
                                delta_time,
                                prev_input_tangent,
                                prev_keyframe_value,
                                prev_output_tangent,
                                next_input_tangent,
                                next_keyframe_value,
                                next_output_tangent,
                            )
                        },
                    };

                    let target = channel.target.read().unwrap();
                    let morph_targets = target.find_components::<MorphTarget>();

                    for morph_target in morph_targets
                    {
                        component_downcast_mut!(morph_target, MorphTarget);

                        for (target_id, weight) in weights.iter().enumerate()
                        {
                            if morph_target.get_data().target_id == target_id as u32
                            {
                                let morph_target_data = morph_target.get_data_mut().get_mut();
                                morph_target_data.weight = *weight * self.weight;
                            }
                        }
                    }
                }
            }
        }

        // ********** apply animation matrix with weight **********
        for (_, target_item) in target_map
        {
            let target_component_arc = target_item.component.clone();
            let mut target_component = target_component_arc.write().unwrap();

            // joint
            if let Some(joint) = target_component.as_any_mut().downcast_mut::<Joint>()
            {
                if target_item.skip_joint
                {
                    continue;
                }

                let joint_id = joint.id();

                let component_data = joint.get_data_mut().get_mut();

                let animation_trans = component_data.animation_trans.as_mut().unwrap();
                let transform = get_animation_transform(&target_item);

                // apply if its the first one
                if approx_zero(component_data.animation_weight) && !approx_zero(self.weight)
                {
                    *animation_trans = transform * self.weight;
                }
                // add if its not the first one
                else if !approx_zero(self.weight)
                {
                    // animation blending - blend this animation with the prev one
                    *animation_trans = *animation_trans + (transform * self.weight);
                }

                component_data.animation_weight += self.weight;
            }
            // transformation
            else if let Some(transformation) = target_component.as_any_mut().downcast_mut::<Transformation>()
            {
                let component_data = transformation.get_data_mut().get_mut();

                if let Some(position) = target_item.position
                {
                    if component_data.animation_position.is_none()
                    {
                        component_data.animation_position = Some(position * self.weight);
                    }
                    else
                    {
                        component_data.animation_position = Some(component_data.animation_position.unwrap() + (position * self.weight));
                    }
                }

                if let Some(rotation_quat) = target_item.rotation_quat
                {
                    if component_data.animation_rotation_quat.is_none()
                    {
                        component_data.animation_rotation_quat = Some(Vector4::<f32>::new(rotation_quat.i * self.weight, rotation_quat.j * self.weight, rotation_quat.k * self.weight, rotation_quat.w * self.weight));
                    }
                    else
                    {
                        let x = component_data.animation_rotation_quat.unwrap().x * rotation_quat.i * self.weight;
                        let y = component_data.animation_rotation_quat.unwrap().y * rotation_quat.j * self.weight;
                        let z = component_data.animation_rotation_quat.unwrap().z * rotation_quat.k * self.weight;
                        let w = component_data.animation_rotation_quat.unwrap().w * rotation_quat.w * self.weight;
                        component_data.animation_rotation_quat = Some(Vector4::<f32>::new(x, y, z, w));
                    }
                }

                if let Some(scale) = target_item.scale
                {
                    if component_data.animation_scale.is_none()
                    {
                        component_data.animation_scale = Some(scale * self.weight);
                    }
                    else
                    {
                        let x = component_data.animation_scale.unwrap().x * scale.x * self.weight;
                        let y = component_data.animation_scale.unwrap().y * scale.y * self.weight;
                        let z = component_data.animation_scale.unwrap().z * scale.z * self.weight;
                        component_data.animation_scale = Some(Vector3::<f32>::new(x, y, z));
                    }
                }

                component_data.animation_weight += self.weight;
                transformation.calc_transform();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        ui.label(format!("Duration: {}", self.to));
        ui.label(format!("Channels: {}", self.channels.len()));

        let mut is_running = self.running();
        let mut is_stopped = !is_running;

        let mut is_pause = self.paused();
        let mut is_reseted = false;

        let icon_size = 20.0;

        ui.horizontal(|ui|
        {
            if ui.toggle_value(&mut is_stopped, RichText::new("⏹").size(icon_size)).on_hover_text("stop animation").clicked()
            {
                self.stop();
            };

            if ui.toggle_value(&mut is_running, RichText::new("⏵").size(icon_size)).on_hover_text("play animation").clicked()
            {
                self.start();
            }

            if ui.toggle_value(&mut is_pause, RichText::new("⏸").size(icon_size)).on_hover_text("pause animation").clicked()
            {
                if self.paused()
                {
                    self.resume();
                }
                else
                {
                    self.pause();
                }
            }

            if ui.toggle_value(&mut is_reseted, RichText::new("⮪").size(icon_size)).on_hover_text("reset animation").clicked()
            {
                self.reset();
            }
        });

        ui.checkbox(&mut self.looped, "Loop");
        ui.checkbox(&mut self.reverse, "Reverse");

        ui.horizontal(|ui|
        {
            ui.label("Easing: ");

            let easings = get_easing_as_string_vec();
            let current_easing_name = easings[self.easing as usize].as_str();
            egui::ComboBox::from_id_source(ui.make_persistent_id("easing_id")).selected_text(current_easing_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap = Some(false);
                ui.set_min_width(30.0);

                let mut current_easing_id = self.easing as usize;

                let mut changed = false;
                for (easing_id, easing) in easings.iter().enumerate()
                {
                    changed = ui.selectable_value(&mut current_easing_id, easing_id, easing).changed() || changed;
                }

                if changed
                {
                    self.easing = Easing::from_repr(current_easing_id).unwrap()
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Weight: ");
            ui.add(egui::Slider::new(&mut self.weight, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Speed: ");
            ui.add(egui::Slider::new(&mut self.speed, 0.0..=10.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("From: ");
            ui.add(egui::Slider::new(&mut self.from, 0.0..=self.to).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("To: ");
            ui.add(egui::Slider::new(&mut self.to, 0.0..=self.duration).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            if !approx_zero(self.to)
            {
                ui.label("Progress: ");
                let mut time = self.animation_time();
                if ui.add(egui::Slider::new(&mut time, 0.0..=self.to).fixed_decimals(2).text("s")).changed()
                {
                    self.set_current_time(time);
                }
            }
        });

        ui.separator();

        // partials
        ui.label("Partial body animation: ");

        let mut delete_id = None;
        for (i, item) in self.joint_filter.iter().enumerate()
        {
            let node = item.0.clone();
            let include = item.1;

            ui.horizontal(|ui|
            {
                let item = node.read().unwrap();

                if include
                {
                    ui.label(RichText::new(format!(" - {} (included): ", item.name)).color(Color32::GREEN));
                }
                else
                {
                    ui.label(RichText::new(format!(" - {} (excluded): ", item.name)).color(Color32::RED));
                }

                if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                {
                    delete_id = Some(i);
                }
            });
        }

        if let Some(delete_id) = delete_id
        {
            self.joint_filter.remove(delete_id);
        }

        if let Some(node) = node
        {
            let node = node.read().unwrap();
            let all_nodes = Scene::list_all_child_nodes(&node.nodes);

            let mut selection: usize = 0;
            let mut changed = false;

            ui.horizontal(|ui|
            {
                ui.label(" - ");

                egui::ComboBox::from_id_source(ui.make_persistent_id("partials")).selected_text("").width(200.0).show_ui(ui, |ui|
                {
                    changed = ui.selectable_value(&mut selection, 0, "").changed() || changed;

                    for (i, child_node) in all_nodes.iter().enumerate()
                    {
                        let child_node = child_node.read().unwrap();
                        if child_node.find_component::<Joint>().is_some()
                        {
                            changed = ui.selectable_value(&mut selection, i + 1, child_node.name.clone()).changed() || changed;
                        }
                    }
                });

                ui.checkbox(&mut self.ui_joint_include_option, "include");
            });

            if changed
            {
                let add_node = &all_nodes[selection - 1];
                self.joint_filter.push((add_node.clone(), self.ui_joint_include_option));
            }
        }

    }
}