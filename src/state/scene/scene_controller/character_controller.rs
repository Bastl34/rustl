use std::{f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{ComplexField, Normed, Point3, Rotation3, Vector2, Vector3};
use parry3d::query::Ray;

use crate::{component_downcast, component_downcast_mut, helper::math::{approx_equal_vec, approx_zero, approx_zero_vec3, yaw_pitch_from_direction}, input::{input_manager::InputManager, keyboard::{Key, Modifier}, mouse::MouseButton}, scene_controller_impl_default, state::scene::{camera_controller::{follow_controller::FollowController, target_rotation_controller::TargetRotationController}, components::{animation::Animation, animation_blending::AnimationBlending, component::ComponentItem, mesh::Mesh, transformation::Transformation, transformation_animation::TransformationAnimation}, manager::id_manager::IdManagerItem, node::{self, Node, NodeItem}, scene::Scene, scene_controller::scene_controller::SceneControllerBase}};

use super::scene_controller::SceneController;

const FADE_SPEED: f32 = 0.1;
//const FADE_SPEED: f32 = 0.15;

const MOVEMENT_SPEED: f32 = 0.03;
const MOVEMENT_SPEED_FAST: f32 = 0.12;

const ROTATION_SPEED: f32 = 0.06;

const CHARACTER_DIRECTION: Vector3<f32> = Vector3::<f32>::new(0.0, 0.0, 1.0);

const GRAVITY: f32 = 0.3;
//const GRAVITY: f32 = 0.981;
//const GRAVITY: f32 = 0.0981;

const FALL_HEIGHT: f32 = 2.0;
const FALL_STOP_HEIGHT: f32 = 0.1;
const BODY_OFFSET: f32 = 0.5;

#[derive(Debug)]
enum CharAnimationType
{
    None,
    Idle,
    Walk,
    Run,
    StrafeLeftWalk,
    StrafeRightWalk,
    StrafeLeftRun,
    StrafeRightRun,
    Jump,
    Crouch,
    Roll,
    Action,
    Fall,
    FallLanding
}

#[derive(PartialEq, Debug)]
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

    pub movement_speed: f32,
    pub movement_speed_fast: f32,

    pub rotation_speed: f32,

    pub rotation_follow: bool,
    pub direction: Vector3<f32>,

    pub gravity: f32,
    pub fall_height: f32,
    pub fall_stop_height: f32,
    pub body_offset: f32,

    pub physics: bool, // very simple at the moment
    pub falling: bool,

    pub strafe: bool,

    pub update_only_on_move: bool,

    node: Option<NodeItem>,
    animation_node: Option<NodeItem>,

    animation_idle: Option<ComponentItem>,
    animation_walk: Option<ComponentItem>,
    animation_run: Option<ComponentItem>,
    animation_jump: Option<ComponentItem>,
    animation_crouch: Option<ComponentItem>,
    animation_roll: Option<ComponentItem>,
    animation_strafe_left_walk: Option<ComponentItem>,
    animation_strafe_right_walk: Option<ComponentItem>,
    animation_strafe_left_run: Option<ComponentItem>,
    animation_strafe_right_run: Option<ComponentItem>,
    animation_fall_idle: Option<ComponentItem>,
    animation_fall_landing: Option<ComponentItem>,

    animation_actions: Vec<ComponentItem>,

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

            movement_speed: MOVEMENT_SPEED,
            movement_speed_fast: MOVEMENT_SPEED_FAST,

            rotation_speed: ROTATION_SPEED,

            rotation_follow: false,
            direction: CHARACTER_DIRECTION,

            gravity: GRAVITY,
            fall_height: FALL_HEIGHT,
            fall_stop_height: FALL_STOP_HEIGHT,
            body_offset: BODY_OFFSET,

            physics: true,
            falling: false,

            strafe: false,

            update_only_on_move: false,

            node: None,
            animation_node: None,

            animation_idle: None,
            animation_walk: None,
            animation_run: None,
            animation_jump: None,
            animation_crouch: None,
            animation_roll: None,
            animation_strafe_left_walk: None,
            animation_strafe_right_walk: None,
            animation_strafe_left_run: None,
            animation_strafe_right_run: None,
            animation_fall_idle: None,
            animation_fall_landing: None,

            animation_actions: vec![],

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


        /*
        let mut follow_controller = FollowController::new();
        follow_controller.data.get_mut().offset.y = 1.0;

        cam.controller = Some(Box::new(follow_controller));
        */

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
                    animation_node.write().unwrap().add_component_front(Arc::new(RwLock::new(Box::new(animation_blending))));

                    self.animation_blending = animation_node.read().unwrap().find_component::<AnimationBlending>();
                }
            }

            let node = node_arc.read().unwrap();

            self.animation_idle = node.find_animation_by_regex("(?i)^idle");
            self.animation_walk = node.find_animation_by_include_exclude(&["walk".to_string()].to_vec(), &["strafe".to_string()].to_vec());
            self.animation_run = node.find_animation_by_include_exclude(&["run".to_string()].to_vec(), &["strafe".to_string()].to_vec());
            self.animation_jump = node.find_animation_by_regex("(?i)jump.*");
            self.animation_crouch = node.find_animation_by_regex("(?i)crouch.*");
            self.animation_roll = node.find_animation_by_regex("(?i)roll.*");
            self.animation_strafe_left_walk = node.find_animation_by_include_exclude(&["strafe".to_string(), "left".to_string(), "walk".to_string()].to_vec(), &vec![]);
            self.animation_strafe_right_walk = node.find_animation_by_include_exclude(&["strafe".to_string(), "right".to_string(), "walk".to_string()].to_vec(), &vec![]);
            self.animation_strafe_left_run = node.find_animation_by_include_exclude(&["strafe".to_string(), "left".to_string(), "run".to_string()].to_vec(), &vec![]);
            self.animation_strafe_right_run = node.find_animation_by_include_exclude(&["strafe".to_string(), "right".to_string(), "run".to_string()].to_vec(), &vec![]);
            self.animation_fall_idle = node.find_animation_by_include_exclude(&["fall".to_string()].to_vec(), &["land".to_string()].to_vec());
            self.animation_fall_landing = node.find_animation_by_include_exclude(&["fall".to_string(), "land".to_string()].to_vec(), &vec![]);

            let actions = vec!["(?i)action.*punch", "(?i)action.*thumbs", "(?i)action.*dance"];

            for action in actions
            {
                let animation = node.find_animation_by_regex(action);

                if let Some(animation) = animation
                {
                    self.animation_actions.push(animation);
                    dbg!(action);
                }
            }
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

        self.start_animation(CharAnimationType::Idle, 0, AnimationMixing::Stop, true, false, false);
    }

    fn get_animation(&self, animation: CharAnimationType, index: usize) -> Option<ComponentItem>
    {
        match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.clone(),
            CharAnimationType::Walk => self.animation_walk.clone(),
            CharAnimationType::Run => self.animation_run.clone(),
            CharAnimationType::StrafeLeftWalk => self.animation_strafe_left_walk.clone(),
            CharAnimationType::StrafeRightWalk => self.animation_strafe_right_walk.clone(),
            CharAnimationType::StrafeLeftRun => self.animation_strafe_left_run.clone(),
            CharAnimationType::StrafeRightRun => self.animation_strafe_right_run.clone(),
            CharAnimationType::Jump => self.animation_jump.clone(),
            CharAnimationType::Crouch => self.animation_crouch.clone(),
            CharAnimationType::Roll => self.animation_roll.clone(),
            CharAnimationType::Fall => self.animation_fall_idle.clone(),
            CharAnimationType::FallLanding => self.animation_fall_landing.clone(),
            CharAnimationType::Action => self.animation_actions.get(index).cloned(),
        }
    }

    fn get_animation_duration(&self, animation: CharAnimationType, index: usize) -> f32
    {
        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.as_ref(),
            CharAnimationType::Walk => self.animation_walk.as_ref(),
            CharAnimationType::Run => self.animation_run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animation_strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animation_strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animation_strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animation_strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animation_jump.as_ref(),
            CharAnimationType::Crouch => self.animation_crouch.as_ref(),
            CharAnimationType::Roll => self.animation_roll.as_ref(),
            CharAnimationType::Fall => self.animation_fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animation_fall_landing.as_ref(),
            CharAnimationType::Action => self.animation_actions.get(index),
        };

        if let Some(animation_item) = animation_item
        {
            component_downcast!(animation_item, Animation);
            return animation_item.to;
        }

        0.0
    }

    fn is_animation_running(&self, animation: CharAnimationType, index: usize) -> bool
    {
        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animation_idle.as_ref(),
            CharAnimationType::Walk => self.animation_walk.as_ref(),
            CharAnimationType::Run => self.animation_run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animation_strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animation_strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animation_strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animation_strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animation_jump.as_ref(),
            CharAnimationType::Crouch => self.animation_crouch.as_ref(),
            CharAnimationType::Roll => self.animation_roll.as_ref(),
            CharAnimationType::Fall => self.animation_fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animation_fall_landing.as_ref(),
            CharAnimationType::Action => self.animation_actions.get(index),
        };

        if let Some(animation_item) = animation_item
        {
            component_downcast!(animation_item, Animation);
            return animation_item.running();
        }

        false
    }

    fn is_any_animation_running(&self) -> bool
    {
        let mut animation_items = vec!
        [
            self.animation_idle.clone(),
            self.animation_walk.clone(),
            self.animation_run.clone(),
            self.animation_strafe_left_walk.clone(),
            self.animation_strafe_right_walk.clone(),
            self.animation_strafe_left_run.clone(),
            self.animation_strafe_right_run.clone(),
            self.animation_jump.clone(),
            self.animation_crouch.clone(),
            self.animation_roll.clone(),
            self.animation_fall_idle.clone(),
            self.animation_fall_landing.clone(),
        ];

        for action in &self.animation_actions
        {
            animation_items.push(Some(action.clone()));
        }

        for animation in animation_items
        {
            if let Some(animation) = animation
            {
                component_downcast!(animation, Animation);
                if animation.running()
                {
                    return true;
                }
            }
        }

        false
    }

    fn get_all_animations_weights(&self) -> f32
    {
        let mut animation_items = vec!
        [
            self.animation_idle.clone(),
            self.animation_walk.clone(),
            self.animation_run.clone(),
            self.animation_strafe_left_walk.clone(),
            self.animation_strafe_right_walk.clone(),
            self.animation_strafe_left_run.clone(),
            self.animation_strafe_right_run.clone(),
            self.animation_jump.clone(),
            self.animation_crouch.clone(),
            self.animation_roll.clone(),
            self.animation_fall_idle.clone(),
            self.animation_fall_landing.clone(),
        ];

        for action in &self.animation_actions
        {
            animation_items.push(Some(action.clone()));
        }

        let mut weight = 0.0;

        for animation in animation_items
        {
            if let Some(animation) = animation
            {
                component_downcast!(animation, Animation);
                if animation.running()
                {
                    weight += animation.weight;
                }
            }
        }

        weight
    }

    fn get_all_running_animations(&self) -> Vec<ComponentItem>
    {
        let mut animation_items = vec!
        [
            self.animation_idle.clone(),
            self.animation_walk.clone(),
            self.animation_run.clone(),
            self.animation_strafe_left_walk.clone(),
            self.animation_strafe_right_walk.clone(),
            self.animation_strafe_left_run.clone(),
            self.animation_strafe_right_run.clone(),
            self.animation_jump.clone(),
            self.animation_crouch.clone(),
            self.animation_roll.clone(),
            self.animation_fall_idle.clone(),
            self.animation_fall_landing.clone(),
        ];

        for action in &self.animation_actions
        {
            animation_items.push(Some(action.clone()));
        }

        let mut animations = vec![];

        for animation in animation_items
        {
            if let Some(animation) = animation
            {
                let animation_clone = animation.clone();

                component_downcast!(animation, Animation);
                if animation.running()
                {
                    animations.push(animation_clone.clone());
                }
            }
        }

        animations
    }

    fn is_jumping(&self) -> bool
    {
        if let Some(animation_jump) = &self.animation_jump
        {
            component_downcast!(animation_jump, Animation);
            return animation_jump.running() && animation_jump.animation_time() < animation_jump.to - self.fade_speed
        }

        false
    }

    fn is_rolling(&self) -> bool
    {
        if let Some(animation_roll) = &self.animation_roll
        {
            component_downcast!(animation_roll, Animation);
            return animation_roll.running() && animation_roll.animation_time() < animation_roll.to - self.fade_speed
        }

        false
    }

    fn is_landing(&self) -> bool
    {
        if let Some(animation_fall_landing) = &self.animation_fall_landing
        {
            component_downcast!(animation_fall_landing, Animation);
            return animation_fall_landing.running() && animation_fall_landing.animation_time() < animation_fall_landing.to - self.fade_speed
        }

        false
    }

    fn is_action(&self) -> bool
    {
        for animation in &self.animation_actions
        {
            component_downcast!(animation, Animation);
            if animation.running() && animation.animation_time() < animation.to - self.fade_speed
            {
                return true;
            }
        }

        false
    }

    fn start_animation(&mut self, animation: CharAnimationType, index: usize, mix_type: AnimationMixing, looped: bool, reverse: bool, reset_time: bool)
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
            CharAnimationType::Idle => self.animation_idle.as_ref(),
            CharAnimationType::Walk => self.animation_walk.as_ref(),
            CharAnimationType::Run => self.animation_run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animation_strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animation_strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animation_strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animation_strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animation_jump.as_ref(),
            CharAnimationType::Crouch => self.animation_crouch.as_ref(),
            CharAnimationType::Roll => self.animation_roll.as_ref(),
            CharAnimationType::Fall => self.animation_fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animation_fall_landing.as_ref(),
            CharAnimationType::Action => self.animation_actions.get(index),
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

            if mix_type == AnimationMixing::Stop
            {
                animation_item.weight = 1.0;
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
        if self.node.is_none()
        {
            return false;
        }

        let node = self.node.clone().unwrap();

        let mut has_change = false;

        let all_keys = vec![Key::W, Key::A, Key::S, Key::D, Key::Space, Key::Escape];
        let mut movement = Vector3::<f32>::zeros();
        let mut rotation = Vector3::<f32>::zeros();

        let mut is_jumping = self.is_jumping();
        let mut is_landing = self.is_landing();
        let mut is_rolling = self.is_rolling();

        let mut is_action = self.is_action();

        // ********** forward/backward **********
        if !input_manager.keyboard.is_holding(Key::C) && !is_action && !is_landing
        {
            if input_manager.keyboard.is_holding(Key::W) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, true, false, false);
                }

                movement.z = self.movement_speed;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::S) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, true, true, false);
                }
                movement.z = -self.movement_speed;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::W) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling
                {
                    self.start_animation(CharAnimationType::Run, 0, AnimationMixing::Fade, true, false, false);
                }

                movement.z = self.movement_speed_fast;
                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::S) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, true, true, false);
                }

                movement.z = -self.movement_speed;
                has_change = true;
            }
        }

        // ********** left/right **********
        if !is_landing
        {
            if input_manager.keyboard.is_holding(Key::A)
            {
                if !self.strafe || input_manager.keyboard.is_holding(Key::W) || input_manager.keyboard.is_holding(Key::S)
                {
                    rotation.y = self.rotation_speed;
                }
                else
                {
                    if input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                    {
                        self.start_animation(CharAnimationType::StrafeLeftRun, 0, AnimationMixing::Fade, true, false, false);
                        movement.x = -self.movement_speed_fast;
                    }
                    else
                    {
                        self.start_animation(CharAnimationType::StrafeLeftWalk, 0, AnimationMixing::Fade, true, false, false);
                        movement.x = -self.movement_speed;
                    }
                }

                has_change = true;
            }
            else if input_manager.keyboard.is_holding(Key::D)
            {
                if !self.strafe || input_manager.keyboard.is_holding(Key::W) || input_manager.keyboard.is_holding(Key::S)
                {
                    rotation.y = -self.rotation_speed;
                }
                else
                {
                    if input_manager.keyboard.is_holding_modifier(Modifier::Shift)
                    {
                        self.start_animation(CharAnimationType::StrafeRightRun, 0, AnimationMixing::Fade, true, false, false);
                        movement.x = self.movement_speed_fast;
                    }
                    else
                    {
                        self.start_animation(CharAnimationType::StrafeRightWalk, 0, AnimationMixing::Fade, true, false, false);
                        movement.x = self.movement_speed;
                    }
                }

                has_change = true;
            }
        }

        // ********** jump **********
        if input_manager.keyboard.is_pressed_no_wait(Key::Space) && !input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) && !input_manager.keyboard.is_holding(Key::C) && !is_jumping && !is_rolling && !is_action && !is_landing && !self.falling
        {
            self.start_animation(CharAnimationType::Jump, 0, AnimationMixing::Fade, false, false, true);
            has_change = true;
        }
        // ********** crouch **********
        else if (input_manager.keyboard.is_holding(Key::C) || input_manager.keyboard.is_holding_modifier(Modifier::Ctrl)) && approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing
        {
            self.start_animation(CharAnimationType::Crouch, 0, AnimationMixing::Fade, false, false, false);
            has_change = true;
        }
        // ********** roll **********
        else if input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) && !approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing && !self.falling
        {
            if movement.z > 0.0
            {
                self.start_animation(CharAnimationType::Roll, 0, AnimationMixing::Fade, false, false, true);
            }
            else
            {
                self.start_animation(CharAnimationType::Roll, 0, AnimationMixing::Fade, false, true, true);
            }

            has_change = true;
        }
        // ********** action **********
        else if approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing
        {
            if input_manager.keyboard.is_pressed_no_wait(Key::Key1) { self.start_animation(CharAnimationType::Action, 0, AnimationMixing::Fade, false, false, true); has_change = true;}
            if input_manager.keyboard.is_pressed_no_wait(Key::Key2) { self.start_animation(CharAnimationType::Action, 1, AnimationMixing::Fade, false, false, true); has_change = true;}
            if input_manager.keyboard.is_pressed_no_wait(Key::Key3) { self.start_animation(CharAnimationType::Action, 2, AnimationMixing::Fade, false, false, true); has_change = true;}
            if input_manager.keyboard.is_pressed_no_wait(Key::Key4) { self.start_animation(CharAnimationType::Action, 3, AnimationMixing::Fade, false, false, true); has_change = true;}
            if input_manager.keyboard.is_pressed_no_wait(Key::Key5) { self.start_animation(CharAnimationType::Action, 4, AnimationMixing::Fade, false, false, true); has_change = true;}
        }
        // ********** stop **********
        else if input_manager.keyboard.is_pressed_no_wait(Key::Escape)
        {
            self.start_animation(CharAnimationType::None, 0, AnimationMixing::Stop, false, false, false);
            has_change = true;
        }

        // ********** refresh states **********
        is_jumping = self.is_jumping();
        is_action = self.is_action();
        is_landing = self.is_landing();
        is_rolling = self.is_rolling();

        // ********** idle **********
        if approx_zero_vec3(&movement) && !self.falling && !is_jumping && !is_rolling && !is_action && !is_landing && !input_manager.keyboard.is_holding_modifier(Modifier::Ctrl) && !input_manager.keyboard.is_holding(Key::C)
        {
            self.start_animation(CharAnimationType::Idle, 0, AnimationMixing::Fade, true, false, false);
        }

        /*
        let weight_combined = self.get_all_animations_weights();

        if weight_combined < 1.0
        {
            if let Some(idle) = self.get_animation(CharAnimationType::Idle, 0)
            {
                component_downcast_mut!(idle, Animation);
                idle.weight += 1.0 - weight_combined;
                idle.start();
            }
        }

        // ********** check animations **********
        let running_animations = self.get_all_running_animations();
        if running_animations.len() == 1
        {
            let animation = running_animations.first().unwrap();
            component_downcast_mut!(animation, Animation);
            animation.weight = 1.0;
        }
         */

        // ********** "physics" **********
        if self.physics && (!approx_zero_vec3(&movement) || !self.update_only_on_move)
        {
            if let Some(transformation) = &self.transformation
            {
                let mut pos;
                let down;
                {
                    component_downcast_mut!(transformation, Transformation);
                    let transform_data = transformation.get_data();

                    pos = Point3::<f32>::new(transform_data.position.x, transform_data.position.y, transform_data.position.z);
                    pos.y += self.body_offset;
                    down = Vector3::new(0.0, -1.0, 0.0);
                }

                let character_node = node.clone();

                let predicate_func = move |node: NodeItem| -> bool
                {
                    let check_node = node.read().unwrap();

                    let is_char_node = check_node.has_parent(character_node.clone());

                    !is_char_node
                };

                let ray = Ray::new(pos, down);
                let pick_res = scene.multi_pick(&ray, false, false, Some(Box::new(predicate_func)));

                if let Some(first_pick) = pick_res.first()
                {
                    let distance = first_pick.0 - self.body_offset;

                    //if self.falling && approx_zero(distance)
                    if self.falling && distance.abs() <= 0.1
                    {
                        self.start_animation(CharAnimationType::FallLanding, 0, AnimationMixing::Fade, false, false, true);
                        self.falling = false;
                    }

                    // move up
                    if distance < 0.0
                    {
                        movement.y -= distance;
                    }
                    // move down
                    else if !is_jumping
                    {
                        if distance > self.fall_height || self.falling
                        {
                            if !is_rolling
                            {
                                self.start_animation(CharAnimationType::Fall, 0, AnimationMixing::Fade, false, false, false);
                            }
                            self.falling = true;
                        }

                        let mut down = self.gravity * frame_scale;

                        if down > distance
                        {
                            down = distance;
                        }

                        movement.y -= down;
                    }
                }
            }
        }

        // ********** apply movement **********
        if !approx_zero_vec3(&movement) || !approx_zero_vec3(&rotation)
        {
            if let Some(transformation) = &self.transformation
            {
                let movement_frame_scale = movement * frame_scale;
                let rotation_frame_scale = rotation * frame_scale;

                component_downcast_mut!(transformation, Transformation);

                transformation.apply_rotation(rotation_frame_scale);

                let rotation_mat = Rotation3::from_axis_angle(&Vector3::y_axis(), transformation.get_data().rotation.y);
                self.direction = (rotation_mat * CHARACTER_DIRECTION).normalize();

                if !approx_zero_vec3(&movement_frame_scale)
                {
                    let movement_in_direction;

                    // strafe left/right
                    if !approx_zero(movement_frame_scale.x)
                    {
                        let rotation_strafe = Rotation3::from_axis_angle(&Vector3::y_axis(), -std::f32::consts::FRAC_PI_2);
                        let strafe_dir = rotation_strafe * self.direction;

                        // strafe left
                        if movement_frame_scale.x < 0.0
                        {
                            movement_in_direction = movement_frame_scale.x * strafe_dir.normalize();
                        }
                        // strafe right
                        else
                        {
                            movement_in_direction = movement_frame_scale.x * strafe_dir.normalize();
                        }
                    }

                    // forward/backward
                    else
                    {
                        movement_in_direction = movement_frame_scale.z * self.direction.normalize();
                    }

                    let gravity = Vector3::<f32>::new(0.0, movement.y, 0.0);

                    let res = movement_in_direction + gravity;

                    transformation.apply_translation(res);
                }
            }
        }

        // ********** camera angle **********
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
            ui.label("Body Offset: ");
            ui.add(egui::Slider::new(&mut self.body_offset, 0.0..=2.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Gravity: ");
            ui.add(egui::Slider::new(&mut self.gravity, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Fall Height: ");
            ui.add(egui::Slider::new(&mut self.fall_height, 0.0..=10.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Fall Stop Height: ");
            ui.add(egui::Slider::new(&mut self.fall_stop_height, 0.001..=1.0).fixed_decimals(3));
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.physics, "Physics (Collide with ground)");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.strafe, "Strafe (Left/Right)");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.update_only_on_move, "Update only on movement");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.rotation_follow, "Rotation Follow");
        });
    }
}