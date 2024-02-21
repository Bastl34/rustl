use std::{f32::consts::PI, sync::{Arc, RwLock}};

use crate::{component_downcast, component_downcast_mut, input::{input_manager::InputManager, keyboard::{Key, Modifier}}, scene_controller_impl_default, state::scene::{camera_controller::target_rotation_controller::TargetRotationController, components::{animation::Animation, animation_blending::AnimationBlending, component::ComponentItem}, manager::id_manager::IdManagerItem, node::{Node, NodeItem}, scene_controller::scene_controller::SceneControllerBase}};

use super::scene_controller::SceneController;


const FADE_SPEED: f32 = 0.1;

enum CharAnimationType
{
    None,
    Idle,
    Walk,
    Run,
    Left,
    Right,
    Jump,
    Crouch
}

#[derive(PartialEq)]
enum AnimationMixing
{
    Stop,
    Fade,
    Mix
}

pub struct CharacterController
{
    base: SceneControllerBase,

    node: Option<NodeItem>,
    animation_node: Option<NodeItem>,

    animation_idle: Option<ComponentItem>,
    animation_walk: Option<ComponentItem>,
    animation_run: Option<ComponentItem>,
    animation_jump: Option<ComponentItem>,
    animation_crouch: Option<ComponentItem>,
    animation_left: Option<ComponentItem>,
    animation_right: Option<ComponentItem>,

    animation_blending: Option<ComponentItem>,
}

impl CharacterController
{
    pub fn default() -> Self
    {
        CharacterController
        {
            base: SceneControllerBase::new("Character Controller".to_string(), "🏃".to_string()),

            node: None,
            animation_node: None,

            animation_idle: None,
            animation_walk: None,
            animation_run: None,
            animation_jump: None,
            animation_crouch: None,
            animation_left: None,
            animation_right: None,

            animation_blending: None,
        }
    }

    pub fn auto_setup(&mut self, scene: &mut crate::state::scene::scene::Scene, id_manager: IdManagerItem, character_node: &str)
    {
        let node = scene.find_node_by_name("avatar2");
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
            self.animation_crouch = node.find_animation_by_regex("(?i)crouch.*");
            self.animation_left = node.find_animation_by_regex("(?i)left");
            self.animation_right = node.find_animation_by_regex("(?i)right");
        }

        self.start_animation(CharAnimationType::Idle, AnimationMixing::Stop, true, false);
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
        };

        if let Some(animation_item) = animation_item
        {
            component_downcast!(animation_item, Animation);
            return animation_item.duration;
        }

        0.0
    }

    fn start_animation(&mut self, animation: CharAnimationType, mix_type: AnimationMixing, looped: bool, reverse: bool)
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
        };

        if mix_type == AnimationMixing::Fade && animation_item.is_some()
        {
            let animation_item = animation_item.clone().unwrap();

            if let Some(animation_blending) = &self.animation_blending
            {
                component_downcast_mut!(animation_blending, AnimationBlending);
                animation_blending.speed = FADE_SPEED;
                animation_blending.to = Some(animation_item.read().unwrap().get_base().id);

                //component_downcast_mut!(animation_item, Animation);
                //animation_item.weight = 0.0;
                //animation_item.start();
            }
        }

        if let Some(animation_item) = animation_item
        {
            component_downcast_mut!(animation_item, Animation);
            animation_item.looped = looped;
            animation_item.reverse = reverse;
            animation_item.start();
        }
    }
}

impl SceneController for CharacterController
{
    scene_controller_impl_default!();

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, input_manager: &mut InputManager, frame_scale: f32) -> bool
    {
        let walk_keys = vec![Key::W, Key::A, Key::S, Key::D];

        let mut animation_running = false;

        //if input_manager.keyboard.is_holding_by_keys(&walk_keys) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        if input_manager.keyboard.is_holding(Key::W) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        {
            self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, false);
            animation_running = true;
            dbg!("start walking");
        }
        else if input_manager.keyboard.is_holding(Key::S) && !input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        {
            self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, true);
            animation_running = true;
            dbg!("start walking");
        }
        else if input_manager.keyboard.is_holding(Key::W) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        {
            self.start_animation(CharAnimationType::Run, AnimationMixing::Fade, true, false);
            animation_running = true;
            dbg!("start running");
        }
        else if input_manager.keyboard.is_holding(Key::S) && input_manager.keyboard.is_holding_modifier(Modifier::Shift)
        {
            self.start_animation(CharAnimationType::Walk, AnimationMixing::Fade, true, true);
            animation_running = true;
            dbg!("start running");
        }
        else if input_manager.keyboard.is_pressed_no_wait(Key::Space)
        {
            self.start_animation(CharAnimationType::Jump, AnimationMixing::Fade, false, false);
            animation_running = true;
            dbg!("start jumping");
        }
        else if input_manager.keyboard.is_pressed_no_wait(Key::Escape)
        {
            //self.start_animation(CharAnimationType::None, AnimationMixing::Stop);
            self.start_animation(CharAnimationType::None, AnimationMixing::Stop, false, false);
            animation_running = true;
            dbg!("start STOP");
        }
        //else
        {
            //self.start_animation(CharAnimationType::Idle, AnimationMixing::Fade, true);
        }

        // check running animations and start idle if needed
        /*
        if !animation_running
        {
            if let Some(animation_node) = &self.animation_node
            {
                for animation in animation_node.read().unwrap().find_components::<Animation>()
                {
                    component_downcast!(animation, Animation);
                    animation_running = animation.running() || animation_running;
                }
            }
        }

        if !animation_running
        {
            self.start_animation(CharAnimationType::Idle, AnimationMixing::Fade, true);
        }
         */

        false
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {

    }
}