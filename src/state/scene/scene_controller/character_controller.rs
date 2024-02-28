use std::{f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{Normed, Rotation3, Vector2, Vector3};

use crate::{component_downcast, component_downcast_mut, helper::math::{approx_equal_vec, approx_zero_vec3, yaw_pitch_from_direction}, input::{input_manager::InputManager, keyboard::{Key, Modifier}, mouse::MouseButton}, scene_controller_impl_default, state::scene::{camera_controller::target_rotation_controller::TargetRotationController, components::{animation::Animation, animation_blending::AnimationBlending, component::ComponentItem, transformation::Transformation, transformation_animation::TransformationAnimation}, manager::id_manager::IdManagerItem, node::{Node, NodeItem}, scene_controller::scene_controller::SceneControllerBase}};

use super::scene_controller::SceneController;


const FADE_SPEED: f32 = 0.1;
const JUMP_TIME_DECREASE_SPEED: f32 = 0.3;

const MOVEMENT_SPEED: f32 = 0.03;
const MOVEMENT_SPEED_FAST: f32 = 0.12;

const ROTATION_SPEED: f32 = 0.06;

const CHARACTER_DIRECTION: Vector3<f32> = Vector3::<f32>::new(0.0, 0.0, 1.0);

#[derive(Debug)]
enum CharAnimationType
{
    None,
    Idle,
    Walk,
    Run,
    Left,
    Right,
    Jump,
    Crouch,
    Roll,
    Punch
}

#[derive(PartialEq)]
enum AnimationMixing
{
    Stop,
    Fade
}

pub struct CharacterController
{
    base: SceneControllerBase,

    pub node_name: String,

    pub fade_speed: f32,
    pub jump_time_decrease_speed: f32,

    pub movement_speed: f32,
    pub movement_speed_fast: f32,

    pub rotation_speed: f32,

    pub rotation_follow: bool,
    pub direction: Vector3<f32>,

    node: Option<NodeItem>,
    animation_node: Option<NodeItem>,

    animation_idle: Option<ComponentItem>,
    animation_walk: Option<ComponentItem>,
    animation_run: Option<ComponentItem>,
    animation_jump: Option<ComponentItem>,
    animation_crouch: Option<ComponentItem>,
    animation_roll: Option<ComponentItem>,
    animation_punch: Option<ComponentItem>,
    animation_left: Option<ComponentItem>,
    animation_right: Option<ComponentItem>,

    animation_blending: Option<ComponentItem>,

    transformation: Option<ComponentItem>
}

impl CharacterController
{
    pub fn default() -> Self
    {
        CharacterController
        {
            base: SceneControllerBase::new("Character Controller".to_string(), "🏃".to_string()),

            node_name: "".to_string(),

            fade_speed: FADE_SPEED,
            jump_time_decrease_speed: JUMP_TIME_DECREASE_SPEED,

            movement_speed: MOVEMENT_SPEED,
            movement_speed_fast: MOVEMENT_SPEED_FAST,

            rotation_speed: ROTATION_SPEED,

            rotation_follow: true,
            direction: CHARACTER_DIRECTION,

            node: None,
            animation_node: None,

            animation_idle: None,
            animation_walk: None,
            animation_run: None,
            animation_jump: None,
            animation_crouch: None,
            animation_roll: None,
            animation_punch: None,
            animation_left: None,
            animation_right: None,

            animation_blending: None,

            transformation: None
        }
    }

    pub fn auto_setup(&mut self, scene: &mut crate::state::scene::scene::Scene, character_node: &str)
    {
        let id_manager = scene.id_manager.clone();
        let node = scene.find_node_by_name(character_node);
        let cam = scene.get_active_camera_mut();

        if node.is_none()
        {
            println!("auto setup failed - node not found");
            return;
        }

        if cam.is_none()
        {
            println!("auto setup failed - camera not found");
            return;
        }

        self.node = Some(node.unwrap());
        let node_arc = self.node.clone().unwrap();
        self.node_name = node_arc.read().unwrap().name.clone();

        let cam = cam.unwrap();
        cam.node = Some(node_arc.clone());

        let mut target_rotation_controller = TargetRotationController::default();
        target_rotation_controller.data.get_mut().alpha = PI;
        target_rotation_controller.data.get_mut().beta = PI / 7.0;
        target_rotation_controller.data.get_mut().radius = 6.0;
        target_rotation_controller.data.get_mut().offset.y = 1.0;

        cam.controller = Some(Box::new(target_rotation_controller));

        {
            // blending node
            self.animation_node = Node::find_animation_node(node_arc.clone());

            if let Some(animation_node) = self.animation_node.clone()
            {
                {
                    let animation_node = animation_node.read().unwrap();

                    self.animation_blending = animation_node.find_component::<AnimationBlending>();
                }

                if self.animation_blending.is_none()
                {
                    let component_id = id_manager.write().unwrap().get_next_component_id();
                    let animation_blending = AnimationBlending::new_empty(component_id, "Animation Blending");
                    animation_node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation_blending))));

                    self.animation_blending = animation_node.read().unwrap().find_component::<AnimationBlending>();
                }
            }

            let node = node_arc.read().unwrap();

            self.animation_idle = node.find_animation_by_regex("(?i)^idle");
            self.animation_walk = node.find_animation_by_regex("(?i)walk.*");
            self.animation_run = node.find_animation_by_regex("(?i)run.*");
            self.animation_jump = node.find_animation_by_regex("(?i)jump.*");
            self.animation_punch = node.find_animation_by_regex("(?i)punch");
            self.animation_crouch = node.find_animation_by_regex("(?i)crouch.*");
            self.animation_roll = node.find_animation_by_regex("(?i)roll.*");
            self.animation_left = node.find_animation_by_regex("(?i)left");
            self.animation_right = node.find_animation_by_regex("(?i)right");
        }

        // transformation animation
        {
            let mut node = node_arc.write().unwrap();

            if node.find_component::<Transformation>().is_none()
            {
                let component_id = id_manager.write().unwrap().get_next_component_id();
                let component = Transformation::identity(component_id, "Transformation");
                node.add_component(Arc::new(RwLock::new(Box::new(component))));
            }
            {
                let transformation = node.find_component::<Transformation>().unwrap();
                self.transformation = Some(transformation.clone());

                component_downcast_mut!(transformation, Transformation);
            }

        }

        self.start_animation(CharAnimationType::Idle, AnimationMixing::Stop, true, false, false);
    }

    fn get_animation_duration(&self, animation: CharAnimationType) -> f32
    {
        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.clone(),
            CharAnimationType::Walk => self.animation_walk.clone(),
            CharAnimationType::Run => self.animation_run.clone(),
            CharAnimationType::Left => self.animation_left.clone(),
            CharAnimationType::Right => self.animation_right.clone(),
            CharAnimationType::Jump => self.animation_jump.clone(),
            CharAnimationType::Crouch => self.animation_crouch.clone(),
            CharAnimationType::Roll => self.animation_roll.clone(),
            CharAnimationType::Punch => self.animation_punch.clone(),
        };

        if let Some(animation_item) = animation_item
        {
            component_downcast!(animation_item, Animation);
            return animation_item.duration;
        }

        0.0
    }

    fn is_animation_running(&self, animation: CharAnimationType) -> bool
    {
        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.clone(),
            CharAnimationType::Walk => self.animation_walk.clone(),
            CharAnimationType::Run => self.animation_run.clone(),
            CharAnimationType::Left => self.animation_left.clone(),
            CharAnimationType::Right => self.animation_right.clone(),
            CharAnimationType::Jump => self.animation_jump.clone(),
            CharAnimationType::Crouch => self.animation_crouch.clone(),
            CharAnimationType::Roll => self.animation_roll.clone(),
            CharAnimationType::Punch => self.animation_punch.clone(),
        };

        if let Some(animation_item) = animation_item
        {
            component_downcast!(animation_item, Animation);
            return animation_item.running();
        }

        false
    }

    fn is_jumping(&self) -> bool
    {
        if let Some(animation_jump) = &self.animation_jump
        {
            component_downcast!(animation_jump, Animation);
            return animation_jump.running() && animation_jump.animation_time() < animation_jump.duration - self.jump_time_decrease_speed
        }

        false
    }

    fn is_rolling(&self) -> bool
    {
        if let Some(animation_roll) = &self.animation_roll
        {
            component_downcast!(animation_roll, Animation);
            return animation_roll.running() && animation_roll.animation_time() < animation_roll.duration - self.fade_speed
        }

        false
    }

    fn is_punching(&self) -> bool
    {
        if let Some(animation_punch) = &self.animation_punch
        {
            component_downcast!(animation_punch, Animation);
            return animation_punch.running() && animation_punch.animation_time() < animation_punch.duration - self.fade_speed
        }

        false
    }

    fn start_animation(&mut self, animation: CharAnimationType, mix_type: AnimationMixing, looped: bool, reverse: bool, reset_time: bool)
    {
        if self.node.is_none()
        {
            return;
        }

        let node = self.node.clone().unwrap();
        let node = node.write().unwrap();

        if mix_type == AnimationMixing::Stop
        {
            node.stop_all_animations();
        }

        // reset fade item
        if let Some(animation_blending) = &self.animation_blending
        {
            component_downcast_mut!(animation_blending, AnimationBlending);
            animation_blending.to = None;
        }

        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.clone(),
            CharAnimationType::Walk => self.animation_walk.clone(),
            CharAnimationType::Run => self.animation_run.clone(),
            CharAnimationType::Left => self.animation_left.clone(),
            CharAnimationType::Right => self.animation_right.clone(),
            CharAnimationType::Jump => self.animation_jump.clone(),
            CharAnimationType::Crouch => self.animation_crouch.clone(),
            CharAnimationType::Roll => self.animation_roll.clone(),
            CharAnimationType::Punch => self.animation_punch.clone(),
        };

        if mix_type == AnimationMixing::Fade && animation_item.is_some()
        {
            let animation_item = animation_item.clone().unwrap();

            if let Some(animation_blending) = &self.animation_blending
            {
                component_downcast_mut!(animation_blending, AnimationBlending);
                animation_blending.speed = self.fade_speed;
                animation_blending.to = Some(animation_item.read().unwrap().get_base().id);
            }
        }

        if let Some(animation_item) = animation_item
        {
            component_downcast_mut!(animation_item, Animation);
            animation_item.looped = looped;
            animation_item.reverse = reverse;

            if reset_time
            {
                animation_item.set_current_time(0.0);
            }
            animation_item.start();
        }
    }
}

impl SceneController for CharacterController
{
    scene_controller_impl_default!();

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, input_manager: &mut InputManager, frame_scale: f32) -> bool
    {
        let mut has_change = false;

        let all_keys = vec![Key::W, Key::A, Key::S, Key::D, Key::Space, Key::Escape];
        let mut movement = Vector3::<f32>::zeros();
        let mut rotation = Vector3::<f32>::zeros();

        // forward/backward
        if !input_manager.keyboard.is_holding(Key::C) && !self.is_punching()
        {
            if input_manager.keyboard.is_holding(Key::W) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !self.is_jumping() && !self.is_rolling() && !self.is_punching()
                {
                    self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, false, false);
                }

                movement.z = self.movement_speed;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::S) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !self.is_jumping() && !self.is_rolling() && !self.is_punching()
                {
                    self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, true, false);
                }
                movement.z = -self.movement_speed;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::W) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !self.is_jumping() && !self.is_rolling() && !self.is_punching()
                {
                    self.start_animation(CharAnimationType::Run, AnimationMixing::Fade, true, false, false);
                }

                movement.z = self.movement_speed_fast;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::S) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !self.is_jumping() && !self.is_rolling() && !self.is_punching()
                {
                    self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, true, false);
                }

                movement.z = -self.movement_speed;
                has_change = true;
            }
        }

        // left/right
        if input_manager.keyboard.is_holding(Key::A)
        {
            rotation.y = self.rotation_speed;
            has_change = true;
        }
        else if input_manager.keyboard.is_holding(Key::D)
        {
            rotation.y = -self.rotation_speed;
            has_change = true;
        }

        // jump
        if input_manager.keyboard.is_pressed_no_wait(Key::Space) && !input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) && !input_manager.keyboard.is_holding(Key::C) && !self.is_jumping() && !self.is_rolling() && !self.is_punching()
        {
            self.start_animation(CharAnimationType::Jump, AnimationMixing::Fade, false, false, true);
            has_change = true;
        }
        // crouch
        else if (input_manager.keyboard.is_holding(Key::C) || input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)) && approx_zero_vec3(&movement) && !self.is_jumping() && !self.is_rolling() && !self.is_punching()
        {
            self.start_animation(CharAnimationType::Crouch, AnimationMixing::Fade, false, false, false);
            has_change = true;
        }
        // roll
        else if input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) && !approx_zero_vec3(&movement) && !self.is_jumping() && !self.is_rolling() && !self.is_punching()
        {
            if movement.z > 0.0
            {
                self.start_animation(CharAnimationType::Roll, AnimationMixing::Fade, false, false, true);
            }
            else
            {
                self.start_animation(CharAnimationType::Roll, AnimationMixing::Fade, false, true, true);
            }

            has_change = true;
        }
        // punch
        else if input_manager.keyboard.is_pressed_no_wait(Key::V) && approx_zero_vec3(&movement) && !self.is_jumping() && !self.is_rolling() && !self.is_punching()
        {
            self.start_animation(CharAnimationType::Punch, AnimationMixing::Fade, false, false, true);
            has_change = true;
        }
        // stop
        else if input_manager.keyboard.is_pressed_no_wait(Key::Escape)
        {
            self.start_animation(CharAnimationType::None, AnimationMixing::Stop, false, false, false);
            has_change = true;
        }

        // idle
        if approx_zero_vec3(&movement) && !self.is_jumping() && !self.is_rolling() && !self.is_punching() && !input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)&& !input_manager.keyboard.is_holding(Key::C)
        {
            self.start_animation(CharAnimationType::Idle, AnimationMixing::Fade, false, false, false);
        }

        // apply movement
        if !approx_zero_vec3(&movement) || !approx_zero_vec3(&rotation)
        {
            if let Some(transformation) = &self.transformation
            {
                let movement = movement * frame_scale;
                let rotation = rotation * frame_scale;

                component_downcast_mut!(transformation, Transformation);

                transformation.apply_rotation(rotation);

                let rotation_mat = Rotation3::from_axis_angle(&Vector3::y_axis(), transformation.get_data().rotation.y);
                self.direction = (rotation_mat * CHARACTER_DIRECTION).normalize();

                if !approx_zero_vec3(&movement)
                {
                    let movement_in_direction = movement.z * self.direction.normalize();
                    transformation.apply_translation(movement_in_direction);
                }
            }
        }

        // camera angle
        if !approx_zero_vec3(&rotation) && self.rotation_follow
        {
            if let Some(cam) = scene.get_active_camera_mut()
            {
                if let Some(controller) = cam.controller.as_mut()
                {
                    if let Some(controller) = controller.as_any_mut().downcast_mut::<TargetRotationController>()
                    {
                        let (yaw, _) = yaw_pitch_from_direction(self.direction);
                        controller.data.get_mut().alpha = yaw + PI;
                    }
                }
            }
        }

        has_change
    }

    fn ui(&mut self, ui: &mut egui::Ui, scene: &mut crate::state::scene::scene::Scene)
    {
        ui.horizontal(|ui|
        {
            ui.label("Character Target Name: ");
            ui.text_edit_singleline(&mut self.node_name);
        });

        ui.vertical(|ui|
        {
            if ui.button("Run Auto Setup").clicked()
            {
                self.auto_setup(scene, self.node_name.clone().as_str());
            }
        });

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label("Animation Fade Speed: ");
            ui.add(egui::Slider::new(&mut self.fade_speed, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Jump Time Decrease: ");
            ui.add(egui::Slider::new(&mut self.jump_time_decrease_speed, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Movement Speed: ");
            ui.add(egui::Slider::new(&mut self.movement_speed, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Movement Speed Fast: ");
            ui.add(egui::Slider::new(&mut self.movement_speed_fast, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Rotation Speed: ");
            ui.add(egui::Slider::new(&mut self.rotation_speed, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.rotation_follow, "Rotation Follow");
        });
    }
}