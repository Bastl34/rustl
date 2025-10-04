#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use std::collections::HashMap;

use egui::{Color32, RichText};
use nalgebra::{Vector3, Vector4, Quaternion, UnitQuaternion};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, FromRepr};

use crate::state::scene::components::joint::{JointLayeredTransformData, JointTransformationData};
use crate::{component_downcast, console_error, console_warning};
use crate::helper::option_or_id::OptionOrId;
use crate::state::state::InputOutput;
use crate::{component_downcast_mut, component_impl_default, component_impl_no_update_instance, helper::{easing::Easing, easing::easing, easing::get_easing_as_string_vec, math::{approx_zero, cubic_spline_interpolate_vec, cubic_spline_interpolate_vec3, cubic_spline_interpolate_vec4, interpolate_vec, interpolate_vec3}}, state::scene::{components::joint::Joint, node::NodeItem, scene::Scene}};
use crate::state::scene::exporter::serialization_helper;

use super::sound::Sound;
use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation, morph_target::MorphTarget};

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Interpolation
{
    Linear,
    Step,
    CubicSpline
}

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr, Serialize, Deserialize)]
pub enum AnimationLayerType
{
    Blend, // Blend with last applied animation/s (or bind pose transform)
    Override, // Override last applied animation/s
    OverrideComponent, // Override last applied animation/s but just component wise (no complete override)
    AdditiveComponentAbsolute, // Additive last applied animation/s but just component wise (no complete override) based on root joint with absolute value
    Additive, // Additive to last applied animation/s - no blending
}

impl AnimationLayerType
{
    pub fn string_vec() -> Vec<String>
    {
        vec!
        [
            "Blend".to_string(),
            "Override".to_string(),
            "OverrideComponent".to_string(),
            "AdditiveComponentAbsolute".to_string(),
            "Additive".to_string()
        ]
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Channel
{
    pub interpolation: Interpolation,
    pub timestamps: Vec<f32>,

    pub transform_translation: Vec<Vector3<f32>>,
    pub transform_rotation: Vec<Vector4<f32>>,
    pub transform_scale: Vec<Vector3<f32>>,
    pub transform_morph: Vec<Vec<f32>>,

    #[serde(serialize_with = "serialization_helper::serialize_node", deserialize_with = "serialization_helper::deserialize_node")]
    pub target: OptionOrId<NodeItem>
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

            target: OptionOrId::Some(target)
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

#[derive(Clone, Serialize, Deserialize)]
struct JointFilter
{
    #[serde(serialize_with = "serialization_helper::serialize_node", deserialize_with = "serialization_helper::deserialize_node")]
    pub node: OptionOrId<NodeItem>,
    pub include: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Animation
{
    base: ComponentBase,

    pub looped: bool,
    pub reverse: bool,

    pub easing: Easing,
    pub layer_type: AnimationLayerType,

    pub from: f32,
    pub to: f32,
    pub duration: f32, // based on animation data (to prevent that the animation is longer as the duration)

    #[serde(skip, default)]
    pub start_time: Option<u128>,

    #[serde(skip, default)]
    pub pause_time: Option<u128>,

    pub weight: f32,
    pub speed: f32,

    #[serde(skip, default)]
    pub channels: Vec<Channel>,

    joint_filter: Vec<JointFilter>, // only apply parts of the animation for specific nodes

    #[serde(serialize_with = "serialization_helper::serialize_node", deserialize_with = "serialization_helper::deserialize_node")]
    pub in_place_joint_node: OptionOrId<NodeItem>, // apply the animation in place (only for the hips)

    pub in_place_axis: Vector3<bool>,

    #[serde(serialize_with = "serialization_helper::serialize_component", deserialize_with = "serialization_helper::deserialize_component")]
    pub sound_component: OptionOrId<ComponentItem>,

    #[serde(skip, default)]
    current_time: u128,

    #[serde(skip, default)]
    current_local_time: f32,

    #[serde(skip, default)]
    current_iteration: u64,

    #[serde(skip, default)]
    ui_joint_include_option: bool
}

impl Default for Animation
{
    fn default() -> Self
    {
        Animation
        {
            base: ComponentBase::new("Animation".to_string(), "Animation".to_string(), "🎞".to_string()),

            looped: true,
            reverse: false,

            easing: Easing::None,
            layer_type: AnimationLayerType::Blend,

            from: 0.0,
            to: 0.0,
            duration: 0.0,

            start_time: None,
            pause_time: None,

            weight: 1.0,
            speed: 1.0,

            channels: vec![],

            joint_filter: vec![],
            in_place_joint_node: OptionOrId::None,
            in_place_axis: Vector3::new(true, true, true),

            sound_component: OptionOrId::None,

            current_time: 0,
            current_local_time: 0.0,
            current_iteration: 0,

            ui_joint_include_option: true
        }
    }
}

impl Animation
{
    pub fn new(name: &str) -> Animation
    {
        let mut animation = Animation::default();
        animation.get_base_mut().name = name.to_string();

        animation
    }

    pub fn new_joint_transform(name: &str, joint_target: NodeItem, transform: Option<Vector3<f32>>, rotation: Option<Vector3<f32>>, scale: Option<Vector3<f32>>) -> Animation
    {
        let mut animation = Animation::default();
        animation.get_base_mut().name = name.to_string();

        let mut channel = Channel::new(joint_target);

        if let Some(transform) = transform
        {
            channel.transform_translation.push(transform);
        }

        if let Some(rotation) = rotation
        {
            let rotation_quat = UnitQuaternion::from_euler_angles(rotation.x, rotation.y, rotation.z);
            let rotation_quat = Vector4::<f32>::new(rotation_quat.coords.x, rotation_quat.coords.y, rotation_quat.coords.z, rotation_quat.coords.w);
            channel.transform_rotation.push(rotation_quat);
        }

        if let Some(scale) = scale
        {
            channel.transform_scale.push(scale);
        }

        channel.timestamps.push(0.0);

        animation.channels.push(channel);

        animation
    }

    pub fn new_joint_transform_quat(name: &str, joint_target: NodeItem, transform: Option<Vector3<f32>>, rotation: Option<nalgebra::Unit<Quaternion<f32>>>, scale: Option<Vector3<f32>>) -> Animation
    {
        let mut animation = Animation::default();
        animation.get_base_mut().name = name.to_string();

        let mut channel = Channel::new(joint_target);

        if let Some(transform) = transform
        {
            channel.transform_translation.push(transform);
        }

        if let Some(rotation) = rotation
        {
            channel.transform_rotation.push(Vector4::<f32>::new(rotation.i, rotation.j, rotation.k, rotation.w));
        }

        if let Some(scale) = scale
        {
            channel.transform_scale.push(scale);
        }

        channel.timestamps.push(0.0);

        animation.channels.push(channel);

        animation
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
        self.current_iteration = 0;

        if let Some(sound) = self.sound_component.as_ref()
        {
            component_downcast_mut!(sound, Sound);
            sound.start();
        }
    }

    pub fn resume(&mut self)
    {
        let time = (self.current_time as f64 - (self.current_local_time as f64 * 1000.0 * 1000.0) * (1.0 / self.speed as f64)) as u128;

        self.start_time = Some(time);
        self.pause_time = None;

        if let Some(sound) = self.sound_component.as_ref()
        {
            component_downcast_mut!(sound, Sound);
            sound.start();
        }
    }

    pub fn stop(&mut self)
    {
        if !self.running()
        {
            return;
        }

        self.start_time = None;
        self.reset();

        if let Some(sound) = self.sound_component.as_ref()
        {
            component_downcast_mut!(sound, Sound);
            sound.stop();
        }
    }

    pub fn stop_without_reset(&mut self)
    {
        if !self.running()
        {
            return;
        }

        if let Some(sound) = self.sound_component.as_ref()
        {
            component_downcast_mut!(sound, Sound);
            sound.stop();
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

            if let Some(sound) = self.sound_component.as_ref()
            {
                component_downcast_mut!(sound, Sound);
                sound.pause();
            }
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
            if let Some(target) = channel.target.as_ref()
            {
                let target = target.write().unwrap();

                if let Some(joint) = target.find_component::<Joint>()
                {
                    component_downcast_mut!(joint, Joint);

                    joint.get_data_mut().get_mut().animation_update_frame = None;
                    joint.get_data_mut().get_mut().animation_transforms.clear();
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
        }

        self.start_time = None;
        self.pause_time = None;
        self.current_time = 0;
        self.current_local_time = 0.0;
        self.current_iteration = 0;
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


#[typetag::serde]
impl Component for Animation
{
    component_impl_default!();
    component_impl_no_update_instance!();

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        // node
        if self.in_place_joint_node.is_ref()
        {
            let node_found = context.nodes.iter().find(|node| node.read().unwrap().uuid == self.in_place_joint_node.id().unwrap());
            if let Some(node) = node_found
            {
                self.in_place_joint_node = OptionOrId::Some(node.clone());
            }
            else
            {
                self.in_place_joint_node = OptionOrId::None;
                console_error!("Animation: Node with id {} not found", self.in_place_joint_node.id().unwrap());
            }
        }

        // sound
        if self.sound_component.is_ref()
        {
            // resolve component
            let component = context.components.iter().find(|c| c.read().unwrap().get_base().uuid == self.sound_component.id().unwrap());
            if let Some(component) = component
            {
                self.sound_component = OptionOrId::Some(component.clone());
            }
            else
            {
                self.sound_component = OptionOrId::None;
                console_error!("Animation: sound_component with id {} not found", self.sound_component.id().unwrap());
            }
        }
        else
        {
            self.sound_component = OptionOrId::None;
            console_error!("Animation: no sound_component found");
        }

        // joint filter
        for joint_filter in &mut self.joint_filter
        {
            if joint_filter.node.is_ref()
            {
                // resolve node
                let node_found = context.nodes.iter().find(|node| node.read().unwrap().uuid == joint_filter.node.id().unwrap());
                if let Some(node) = node_found
                {
                    joint_filter.node = OptionOrId::Some(node.clone());
                }
                else
                {
                    joint_filter.node = OptionOrId::None;
                    console_error!("Animation: JointFilter node with id {} not found", joint_filter.node.id().unwrap());
                }
            }
        }

        // channels
        for channel in &mut self.channels
        {
            if channel.target.is_ref()
            {
                // resolve node
                let node_found = context.nodes.iter().find(|node| node.read().unwrap().uuid == channel.target.id().unwrap());
                if let Some(node) = node_found
                {
                    channel.target = OptionOrId::Some(node.clone());
                }
                else
                {
                    channel.target = OptionOrId::None;
                    console_error!("Animation: Channel target with id {} not found", channel.target.id().unwrap());
                }
            }
        }
    }

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
        // joints
        self.joint_filter.retain(|joint|
        {
            if joint.node.is_none()
            {
                return true;
            }
            joint.node.as_ref().unwrap().read().unwrap().id != node.read().unwrap().id
        });

        // in place
        if self.in_place_joint_node.is_some() && self.in_place_joint_node.clone().unwrap().read().unwrap().id == node.read().unwrap().id
        {
            self.in_place_joint_node = OptionOrId::None;
        }

        // channels
        let channels_amount = self.channels.len();

        self.channels.retain(|channel|
        {
            if let Some(target) = channel.target.as_ref()
            {
                return target.read().unwrap().id != node.read().unwrap().id
            }
            true
        });

        channels_amount != self.channels.len()
    }

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        let source = self.as_any().downcast_ref::<Animation>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let animation = Animation
        {
            base: ComponentBase::duplicate(source.get_base()),

            looped: self.looped,
            reverse: self.reverse,

            in_place_joint_node: self.in_place_joint_node.clone(),
            in_place_axis: self.in_place_axis,

            easing: self.easing,
            layer_type: self.layer_type.clone(),

            from: self.from,
            to: self.to,
            duration: self.duration,

            start_time: self.start_time,
            pause_time: self.pause_time,

            weight: self.weight,
            speed: self.speed,

            channels: self.channels.clone(),

            joint_filter: self.joint_filter.clone(),

            sound_component: self.sound_component.clone(),

            current_time: 0,
            current_local_time: 0.0,
            current_iteration: 0,

            ui_joint_include_option: self.ui_joint_include_option
        };

        Some(Arc::new(RwLock::new(Box::new(animation))))
    }

    fn update(&mut self, _node: NodeItem, _io: &mut InputOutput, time: u128, _frame_scale: f32, frame: u64)
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
            let iteration = (t / delta).floor() as u64;

            if iteration != self.current_iteration && self.sound_component.is_some()
            {
                let sound = self.sound_component.as_ref().unwrap();
                component_downcast_mut!(sound, Sound);
                sound.stop();
                sound.set_current_time(t % delta);
                sound.start();
            }

            self.current_iteration = iteration;

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

        // ********** reset joints and transforms (if needed) **********
        for channel in &self.channels
        {
            if channel.target.is_none()
            {
                continue;
            }
            let target = channel.target.as_ref().unwrap();
            let target = target.write().unwrap();

            let joint = target.find_component::<Joint>();
            let transformation = target.find_component::<Transformation>();

            if let Some(joint) = joint
            {
                let joint_clone = joint.clone();

                component_downcast_mut!(joint, Joint);

                let data = joint.get_data_mut().get_mut();

                // no override check is needed here -> this is done in joint
                if data.animation_update_frame == None || data.animation_update_frame.unwrap() != frame
                {
                    joint.get_data_mut().get_mut().animation_update_frame = Some(frame);

                    joint.get_data_mut().get_mut().animation_transforms.clear();
                }

                target_map.insert(joint.id(), TargetMapItem{ component: joint_clone, position: None, rotation_quat: None, scale: None, skip_joint: false });
            }
            else if let Some(transformation) = transformation
            {
                let transformation_clone = transformation.clone();

                component_downcast_mut!(transformation, Transformation);

                let data = transformation.get_data_mut().get_mut();

                if data.animation_update_frame == None || data.animation_update_frame.unwrap() != frame || self.layer_type == AnimationLayerType::Override
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

            if channel.target.is_none()
            {
                // NOT SUPPORTED
                console_warning!("empty animation target is not supported");
                continue;
            }
            let target = channel.target.as_ref().unwrap();

            let has_include_filter = self.joint_filter.iter().any(|joint_filter| joint_filter.include);

            for joint_filter in &self.joint_filter
            {
                let node = joint_filter.node.as_ref();

                if node.is_some() && target.read().unwrap().has_parent_or_is_equal(node.unwrap().clone())
                {
                    if joint_filter.include
                    {
                        joint_included_found = true;
                    }
                    else
                    {
                        joint_excluded_found = true;
                    }
                }
                else if node.is_none() && target.read().unwrap().id == 0 // root node
                {
                    if joint_filter.include
                    {
                        joint_included_found = true;
                    }
                    else
                    {
                        joint_excluded_found = true;
                    }
                }
            }

            let mut skip_joint = if has_include_filter { true } else { false };

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
                let target = target.read().unwrap();
                joint = target.find_component::<Joint>();
            }

            let transformation;
            {
                let target = target.read().unwrap();
                transformation = target.find_component::<Transformation>();
            }

            if joint.is_none() && transformation.is_none()
            {
                // NOT SUPPORTED
                console_warning!("empty joint and transform is not supported for now");
                continue;
            }

            let target_node_id;
            {
                let target = target.read().unwrap();
                target_node_id = target.id;
            }

            let mut target_component_id = 0;
            if let Some(joint) = &joint
            {
                target_component_id = joint.read().unwrap().id();
            } else if let Some(transformation) = transformation
            {
                target_component_id = transformation.read().unwrap().id();
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

                    let target = target.read().unwrap();
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

                apply_transformation_to_target(&mut target_map, target_component_id, &transform);
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
                    let is_in_place = self.in_place_joint_node.is_some() && self.in_place_joint_node.clone().unwrap().read().unwrap().id == target_node_id;

                    // in place check
                    let mut in_place_local_transform = Vector3::<f32>::new(0.0, 0.0, 0.0);
                    if is_in_place
                    {
                        let joint = joint.unwrap();
                        component_downcast!(joint, Joint);
                        let local_transform = joint.get_local_transform();

                        in_place_local_transform.x = local_transform[(0, 3)];
                        in_place_local_transform.y = local_transform[(1, 3)];
                        in_place_local_transform.z = local_transform[(2, 3)];
                    }

                    // interpolation
                    let translation_interpolated = match channel.interpolation
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

                    let mut translation = translation_interpolated;
                    if is_in_place && self.in_place_axis.x { translation.x = in_place_local_transform.x; }
                    if is_in_place && self.in_place_axis.y { translation.y = in_place_local_transform.y; }
                    if is_in_place && self.in_place_axis.z { translation.z = in_place_local_transform.z; }

                    apply_transformation_to_target(&mut target_map, target_component_id, &(Some(translation), None, None));
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

                    apply_transformation_to_target(&mut target_map, target_component_id, &(None, Some(rotation), None));
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

                    apply_transformation_to_target(&mut target_map, target_component_id, &(None, None, Some(scale)));
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

                    let target = target.read().unwrap();
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

            // skip joint flag
            if let Some(target) = target_map.get_mut(&target_component_id)
            {
                target.skip_joint = skip_joint;
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

                let component_data = joint.get_data_mut().get_mut();

                component_data.animation_transforms.push
                (
                    JointLayeredTransformData
                    {
                        layer_type: self.layer_type,
                        transformation: JointTransformationData
                        {
                            translation: target_item.position,
                            rotation_quat: target_item.rotation_quat,
                            scale: target_item.scale,
                        },
                        weight: self.weight,
                    }
                );
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

        let mut joint_targets = 0;
        let mut transformation_targets = 0;

        for channel in &self.channels
        {
            if let Some(target) = channel.target.as_ref()
            {
                let target = target.write().unwrap();

                if target.has_component::<Joint>()
                {
                    joint_targets += 1;
                }
                else if target.has_component::<Transformation>()
                {
                    transformation_targets += 1;
                }
            }
        }

        ui.label(format!("Duration: {}", self.to));
        ui.label(format!("Channels: {} (Joints: {}, Transform {})", self.channels.len(), joint_targets, transformation_targets));

        let mut is_running = self.running();
        let mut is_stopped = !is_running;

        let mut is_pause = self.paused();
        let mut is_reseted = false;

        let icon_size = 20.0;

        // ********** controls **********
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


        // ********** settings **********
        ui.checkbox(&mut self.looped, "Loop");
        ui.checkbox(&mut self.reverse, "Reverse");

        ui.horizontal(|ui|
        {
            ui.label("Layer Type: ");

            let layer_types = AnimationLayerType::string_vec();
            let current_layer_type = layer_types[self.layer_type as usize].clone();
            egui::ComboBox::from_id_salt(ui.make_persistent_id("layer_type_id")).selected_text(current_layer_type).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.set_min_width(30.0);

                let mut current_layer_type_id = self.layer_type as usize;

                let mut changed = false;
                for (layer_type_id, layer_type) in layer_types.iter().enumerate()
                {
                    changed = ui.selectable_value(&mut current_layer_type_id, layer_type_id, layer_type.clone()).changed() || changed;
                }

                if changed
                {
                    self.layer_type = AnimationLayerType::from_repr(current_layer_type_id).unwrap()
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Easing: ");

            let easings = get_easing_as_string_vec();
            let current_easing_name = easings[self.easing as usize].as_str();
            egui::ComboBox::from_id_salt(ui.make_persistent_id("easing_id")).selected_text(current_easing_name).show_ui(ui, |ui|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
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
                if ui.add(egui::Slider::new(&mut time, 0.0..=self.to).fixed_decimals(2).clamping(egui::SliderClamping::Edits).text("s")).changed()
                {
                    self.set_current_time(time);
                }
            }
        });

        ui.separator();

        // ********** in place **********
        ui.horizontal(|ui|
        {
            ui.label("In Place Joint: ");
            if let Some(in_place_joint_node) = self.in_place_joint_node.as_ref().cloned()
            {
                let in_place_joint_node = in_place_joint_node.read().unwrap();
                ui.label(in_place_joint_node.name.clone());

                if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                {
                    self.in_place_joint_node = OptionOrId::None;
                }
            }
            else if let Some(node) = node.clone()
            {
                let node = node.read().unwrap();
                let all_nodes = Scene::list_all_child_nodes(&node.nodes);

                let mut selection: usize = 0;
                let mut changed = false;

                ui.horizontal(|ui|
                {
                    egui::ComboBox::from_id_salt(ui.make_persistent_id("in_place")).selected_text("").width(200.0).show_ui(ui, |ui|
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
                });

                if changed
                {
                    let add_node = &all_nodes[selection - 1];
                    self.in_place_joint_node = OptionOrId::Some(add_node.clone());
                }
            }
        });

        ui.add_enabled_ui(self.in_place_joint_node.is_some(), |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("Axes: ");
                ui.checkbox(&mut self.in_place_axis.x, "x");
                ui.checkbox(&mut self.in_place_axis.y, "y");
                ui.checkbox(&mut self.in_place_axis.z, "z");
            });
        });

        ui.separator();

        // ********** partials **********
        ui.label("Partial body animation: ");

        let mut delete_id = None;
        for (i, item) in self.joint_filter.iter().enumerate()
        {
            if item.node.is_none()
            {
                continue;
            }
            let node = item.node.as_ref().unwrap();
            let include = item.include;

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

        if let Some(node) = &node
        {
            let node = node.read().unwrap();
            let all_nodes = Scene::list_all_child_nodes(&node.nodes);

            let mut selection: usize = 0;
            let mut changed = false;

            ui.horizontal(|ui|
            {
                ui.label(" - ");

                egui::ComboBox::from_id_salt(ui.make_persistent_id("partials")).selected_text("").width(200.0).show_ui(ui, |ui|
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
                self.joint_filter.push(JointFilter { node: OptionOrId::Some(add_node.clone()), include: self.ui_joint_include_option });
            }
        }

        ui.separator();

        // ********** sound **********
        ui.horizontal(|ui|
        {
            ui.label("Sound: ");
            if let Some(sound_component) = self.sound_component.as_ref().cloned()
            {
                let sound_component = sound_component.read().unwrap();
                ui.label(sound_component.get_base().name.clone());

                if ui.button(RichText::new("🗑").color(Color32::LIGHT_RED)).clicked()
                {
                    self.sound_component = OptionOrId::None;
                }

            }
            else if let Some(node) = node.clone()
            {
                let node = node.read().unwrap();
                let sounds = node.find_components::<Sound>();

                let mut selection: usize = 0;
                let mut changed = false;

                ui.horizontal(|ui|
                {
                    egui::ComboBox::from_id_salt(ui.make_persistent_id("sound")).selected_text("").width(200.0).show_ui(ui, |ui|
                    {
                        changed = ui.selectable_value(&mut selection, 0, "").changed() || changed;

                        for (i, sound) in sounds.iter().enumerate()
                        {
                            let sound = sound.read().unwrap();
                            changed = ui.selectable_value(&mut selection, i + 1, sound.get_base().name.clone()).changed() || changed;
                        }
                    });
                });

                if changed
                {
                    let souond_component = &sounds[selection - 1];
                    self.sound_component = OptionOrId::Some(souond_component.clone());
                }
            }
        });

        ui.separator();

    }
}