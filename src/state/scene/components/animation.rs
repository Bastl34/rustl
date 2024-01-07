use std::collections::HashMap;

use egui::RichText;
use nalgebra::{Matrix4, Vector3, Vector4, Quaternion, UnitQuaternion, Rotation3};

use crate::{helper::math::{interpolate_vec3, approx_zero, cubic_spline_interpolate_vec3, cubic_spline_interpolate_vec4}, component_impl_default, state::scene::{node::NodeItem, components::joint::{Joint, self}}, component_impl_no_update_instance, input::input_manager::InputManager, component_downcast_mut};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};

#[derive(PartialEq)]
pub enum Interpolation
{
    Linear,
    Step,
    CubicSpline
}


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

pub struct Animation
{
    base: ComponentBase,

    pub looped: bool,

    pub duration: f32,
    pub start_time: Option<u128>,
    pub pause_time: Option<u128>,

    pub weight: f32,
    pub speed: f32,

    pub channels: Vec<Channel>,

    current_time: u128,
    current_local_time: f32
}

impl Animation
{
    pub fn new(id: u64, name: &str) -> Animation
    {
        Animation
        {
            base: ComponentBase::new(id, name.to_string(), "Animation".to_string(), "🎞".to_string()),

            looped: false,

            duration: 0.0,
            start_time: None,
            pause_time: None,

            weight: 1.0,
            speed: 1.0,

            channels: vec![],

            current_time: 0,
            current_local_time: 0.0
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

        1.0 / self.duration * self.current_local_time
    }

    pub fn animation_time(&self) -> f32
    {
        self.current_local_time % self.duration
    }

    pub fn start(&mut self)
    {
        self.start_time = Some(0);
        self.pause_time = None;
    }

    pub fn resume(&mut self)
    {
        //let time = self.current_time - (self.current_local_time as f64 * 1000.0 * 1000.0) as u128;
        let time = (self.current_time as f64 - (self.current_local_time as f64 * 1000.0 * 1000.0) * (1.0 / self.speed as f64)) as u128;

        self.start_time = Some(time);
        self.pause_time = None;
    }

    pub fn stop(&mut self)
    {
        self.start_time = None;
        self.reset();
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
        self.current_local_time = time % self.duration;
        self.resume();
    }

    pub fn set_speed(&mut self, speed: f32)
    {
        self.speed = speed;
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

                transformation.get_data_mut().get_mut().animation_trans = None;
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
}

impl Component for Animation
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
        }
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
        if let Some(start_time) = self.start_time
        {
            let local_timestamp = ((time - start_time) as f64 / 1000.0 / 1000.0) as f32;
            self.current_local_time = local_timestamp * self.speed;

            if !self.looped && self.current_local_time > self.duration
            {
                self.stop();
                return;
            }

            let mut target_map: HashMap<u64, (ComponentItem, Matrix4::<f32>)> = HashMap::new();

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

                    target_map.insert(joint.id(), (joint_clone, Matrix4::<f32>::identity()));
                }
                else if let Some(transformation) = transformation
                {
                    let transformation_clone = transformation.clone();

                    component_downcast_mut!(transformation, Transformation);

                    let data = transformation.get_data_mut().get_mut();

                    if data.animation_update_frame == None || data.animation_update_frame.unwrap() != frame
                    {
                        transformation.get_data_mut().get_mut().animation_trans = Some(Matrix4::<f32>::identity());
                        transformation.get_data_mut().get_mut().animation_update_frame = Some(frame);
                        transformation.get_data_mut().get_mut().animation_weight = 0.0;
                    }

                    target_map.insert(transformation.id(), (transformation_clone, Matrix4::<f32>::identity()));
                }
            }

            // ********** calculate animation matrix **********
            for channel in &self.channels
            {
                let joint;
                {
                    let target = channel.target.read().unwrap();
                    joint = target.find_component::<Joint>();

                    /*
                    if joint.is_some()
                    {
                        println!("joint target: {} {:?}", target.id, &target.name);
                    }
                    */
                }

                let transformation;
                {
                    let target = channel.target.read().unwrap();

                    transformation = target.find_component::<Transformation>();

                    /*
                    if joint.is_some()
                    {
                        println!("transformation target: {} {:?}", target.id, &target.name);
                    }
                    */
                }

                if joint.is_none() && transformation.is_none()
                //if joint.is_none()
                {
                    // NOT SUPPORTED
                    dbg!("not supported for now");
                    continue;
                }

                let mut target_id = 0;
                if let Some(joint) = joint
                {
                    target_id = joint.read().unwrap().id();
                } else if let Some(transformation) = transformation
                {
                    target_id = transformation.read().unwrap().id();
                }

                // ********** only one item per channel **********
                if channel.timestamps.len() == 0
                {
                    let mut transform = None;
                    if channel.transform_translation.len() > 0
                    {
                        let t = &channel.transform_translation[0];
                        transform = Some(nalgebra::Isometry3::translation(t.x, t.y, t.z).to_homogeneous());
                    }
                    else if channel.transform_rotation.len() > 0
                    {
                        let r = &channel.transform_rotation[0];
                        let quaternion = UnitQuaternion::new_normalize(Quaternion::new(r.w, r.x, r.y, r.z));
                        let quaternion: Rotation3<f32> = quaternion.into();
                        transform = Some(quaternion.to_homogeneous());
                    }
                    else if channel.transform_scale.len() > 0
                    {
                        let s = &channel.transform_scale[0];
                        transform = Some(Matrix4::new_nonuniform_scaling(&s));
                    }
                    else if channel.transform_morph.len() > 0
                    {
                        // TODO
                    }

                    if let Some(transform) = transform
                    {
                        let target_item = target_map.get_mut(&target_id).unwrap();
                        target_item.1 = target_item.1 * transform;
                    }
                }
                // ********** some items per channel **********
                else
                {
                    let min = channel.timestamps[0];
                    let len = channel.timestamps.len();
                    let max = channel.timestamps[len - 1];

                    let interval = max - min;
                    //let t = if self.current_local_time > min { (self.current_local_time - min) % interval + min } else { self.current_local_time };

                    let mut t = self.current_local_time;

                    // some checks
                    if t > max && self.looped
                    {
                        t = t % interval;
                    }
                    else if t > max && !self.looped
                    {
                        t = max;
                    }

                    if t < min
                    {
                        t = min;
                    }

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
                        let transform = match channel.interpolation
                        {
                            Interpolation::Linear =>
                            {
                                let from = &channel.transform_translation[t0];
                                let to = &channel.transform_translation[t1];

                                let interpolated = interpolate_vec3(&from, &to, factor);
                                nalgebra::Isometry3::translation(interpolated.x, interpolated.y, interpolated.z).to_homogeneous()
                            },
                            Interpolation::Step =>
                            {
                                let from = &channel.transform_translation[t0];
                                nalgebra::Isometry3::translation(from.x, from.y, from.z).to_homogeneous()
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

                                nalgebra::Isometry3::translation(res.x, res.y, res.z).to_homogeneous()
                            },
                        };

                        let target_item = target_map.get_mut(&target_id).unwrap();
                        target_item.1 = target_item.1 * transform;
                    }
                    // ********** rotation **********
                    else if channel.transform_rotation.len() > 0
                    {
                        let transform = match channel.interpolation
                        {
                            Interpolation::Linear =>
                            {
                                let from = &channel.transform_rotation[t0];
                                let to = &channel.transform_rotation[t1];

                                let quaternion0 = UnitQuaternion::new_normalize(Quaternion::new(from.w, from.x, from.y, from.z));
                                let quaternion1 = UnitQuaternion::new_normalize(Quaternion::new(to.w, to.x, to.y, to.z));

                                let interpolated = quaternion0.slerp(&quaternion1, factor);
                                let interpolated: Rotation3<f32> = interpolated.into();
                                interpolated.to_homogeneous()
                            },
                            Interpolation::Step =>
                            {
                                let from = &channel.transform_rotation[t0];
                                let quaternion = UnitQuaternion::new_normalize(Quaternion::new(from.w, from.x, from.y, from.z));
                                let quaternion: Rotation3<f32> = quaternion.into();
                                quaternion.to_homogeneous()
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

                                let quaternion = UnitQuaternion::new_normalize(Quaternion::new(res.w, res.x, res.y, res.z));
                                let quaternion: Rotation3<f32> = quaternion.into();
                                quaternion.to_homogeneous()
                            },
                        };

                        let target_item = target_map.get_mut(&target_id).unwrap();
                        target_item.1 = target_item.1 * transform;
                    }
                    // ********** scale **********
                    else if channel.transform_scale.len() > 0
                    {
                        let transform = match channel.interpolation
                        {
                            Interpolation::Linear =>
                            {
                                let from = &channel.transform_scale[t0];
                                let to = &channel.transform_scale[t1];

                                let interpolated = interpolate_vec3(&from, &to, factor);
                                Matrix4::new_nonuniform_scaling(&interpolated)
                            },
                            Interpolation::Step =>
                            {
                                let from = &channel.transform_scale[t0];
                                Matrix4::new_nonuniform_scaling(&from)
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

                                Matrix4::new_nonuniform_scaling(&res)
                            },
                        };

                        let target_item = target_map.get_mut(&target_id).unwrap();
                        target_item.1 = target_item.1 * transform;
                    }
                }
            }

            // ********** apply animation matrix with weight **********
            for (_, target_item) in target_map
            {
                let target_component_arc = target_item.0;
                let mut target_component = target_component_arc.write().unwrap();

                // joint
                if let Some(joint) = target_component.as_any_mut().downcast_mut::<Joint>()
                {
                    let component_data = joint.get_data_mut().get_mut();

                    let animation_trans = component_data.animation_trans.as_mut().unwrap();

                    // apply if its the first one
                    if approx_zero(component_data.animation_weight) && !approx_zero(self.weight)
                    {
                        *animation_trans = target_item.1 * self.weight;
                    }
                    // add if its not the first one
                    else if !approx_zero(self.weight)
                    {
                        // animation blending - blend this animation with the prev one
                        *animation_trans = *animation_trans + (target_item.1 * self.weight);
                    }

                    component_data.animation_weight += self.weight;
                }
                // transformation
                else if let Some(transformation) = target_component.as_any_mut().downcast_mut::<Transformation>()
                {
                    let component_data = transformation.get_data_mut().get_mut();

                    let animation_trans = component_data.animation_trans.as_mut().unwrap();

                    // apply if its the first one
                    if approx_zero(component_data.animation_weight) && !approx_zero(self.weight)
                    {
                        *animation_trans = target_item.1 * self.weight;
                    }
                    // add if its not the first one
                    else if !approx_zero(self.weight)
                    {
                        // animation blending - blend this animation with the prev one
                        *animation_trans = *animation_trans + (target_item.1 * self.weight);
                    }

                    component_data.animation_weight += self.weight;
                    transformation.calc_transform();
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.label(format!("Duration: {}", self.duration));
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
            ui.label("Progress: ");
            let mut time = self.animation_time();
            if ui.add(egui::Slider::new(&mut time, 0.0..=self.duration).fixed_decimals(2).text("s")).changed()
            {
                self.set_current_time(time);
            }
        });
    }
}