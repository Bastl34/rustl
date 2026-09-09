#![allow(dead_code)]

use std::{collections::HashSet, f32::consts::PI, sync::{Arc, RwLock}};

use nalgebra::{Rotation3, Vector3, Vector4};
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::{Capsule, Collider, ColliderHandle, Pose, QueryFilter, Vector};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::console_warning;
use crate::{component_downcast, component_downcast_mut, console_error, console_log, helper::{math::{approx_equal, approx_zero, approx_zero_vec3, extract_translation_from_transform, shortest_angle_dist, yaw_pitch_from_direction}, option_or_id::OptionOrId}, input::keyboard::{Key, Modifier}, scene_controller_impl_default, state::{scene::{camera_controller::target_rotation_controller::TargetRotationController, components::{animation::Animation, animation_blending::AnimationBlending, component::{Component, ComponentItem}, joint::Joint, transformation::Transformation}, node::{Node, NodeItem}, scene::Scene, scene_controller::scene_controller::SceneControllerBase}, state::{get_delta_t, InputOutput, RunMode}}};

use super::scene_controller::SceneController;

const FADE_SPEED: f32 = 0.1;
//const FADE_SPEED: f32 = 0.15;

const MOVEMENT_SPEED: f32 = 0.03;
const MOVEMENT_SPEED_FAST: f32 = 0.12;
const FLY_SPEED: f32 = 0.09;
const FLY_SPEED_FAST: f32 = 0.24;

const ROTATION_SPEED: f32 = 0.06;

const CHARACTER_DIRECTION: Vector3<f32> = Vector3::<f32>::new(0.0, 0.0, -1.0);

const EARTH_GRAVITY: f32 = 9.81;
const JUMP_FORCE: f32 = 5.0;
const MAX_FALL_SPEED: f32 = 30.0; // maybe 50 which is more like the real world max human fall velocity

const FALL_VELOCITY: f32 = 6.0;
const FALL_STOP_HEIGHT: f32 = 0.1;

// collision capsule defaults - auto_setup replaces them with values derived from the bounding box
const CAPSULE_RADIUS: f32 = 0.3;
const CAPSULE_HALF_HEIGHT: f32 = 0.5;
const CAPSULE_CENTER_OFFSET: f32 = 0.8;

const COLLISION_OFFSET: f32 = 0.001;
const SNAP_TO_GROUND: f32 = crate::state::scene::physics::physics_world::SNAP_TO_GROUND_LIMIT;

// A standing character moving more than this in a frame is a hop, not settling.
const STANDING_HOP_LIMIT: f32 = 0.002;

const EYE_OFFSET: f32 = 1.6;
const FOLLOW_OFFSET: f32 = 1.0;
const AUTOSTEP_HEIGHT: f32 = 0.3;
const AUTOSTEP_MIN_WIDTH: f32 = 0.15;
const MAX_SLOPE_CLIMB_ANGLE: f32 = PI / 4.0;
const MIN_SLOPE_SLIDE_ANGLE: f32 = PI / 4.0;

// extra reach of the downwards probe that looks for the collider the character stands on
const GROUND_PROBE_MARGIN: f32 = 0.2;

const DEFAULT_CAM_RADIUS: f32 = 6.0;

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

pub struct AnimationComponents
{
    idle: Option<ComponentItem>,
    walk: Option<ComponentItem>,
    run: Option<ComponentItem>,
    jump: Option<ComponentItem>,
    crouch: Option<ComponentItem>,
    roll: Option<ComponentItem>,
    strafe_left_walk: Option<ComponentItem>,
    strafe_right_walk: Option<ComponentItem>,
    strafe_left_run: Option<ComponentItem>,
    strafe_right_run: Option<ComponentItem>,
    fall_idle: Option<ComponentItem>,
    fall_landing: Option<ComponentItem>,

    actions: Vec<ComponentItem>,
    blending: Option<ComponentItem>,
}

impl Default for AnimationComponents
{
    fn default() -> Self
    {
        Self
        {
            idle: None,
            walk: None,
            run: None,
            jump: None,
            crouch: None,
            roll: None,
            strafe_left_walk: None,
            strafe_right_walk: None,
            strafe_left_run: None,
            strafe_right_run: None,
            fall_idle: None,
            fall_landing: None,

            actions: vec![],

            blending: None,
        }
    }
}

fn serialize_node<S>(node: &OptionOrId<NodeItem>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match node
    {
        OptionOrId::Some(node_item) =>
        {
            let guard = node_item.read().map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(&guard.uuid)
        }
        OptionOrId::Id(uuid) =>
        {
            serializer.serialize_str(uuid)
        }
        OptionOrId::None => serializer.serialize_none(),
    }
}


pub fn deserialize_node<'de, D>(deserializer: D) -> Result<OptionOrId<NodeItem>, D::Error>
where
    D: Deserializer<'de>,
{
    let uuid_opt = Option::<String>::deserialize(deserializer)?;

    if let Some(uuid) = uuid_opt
    {
        Ok(OptionOrId::from_id(uuid))
    }
    else
    {
        Ok(OptionOrId::None)
    }
}

// Settings for how the animation clips interact with the controller driven movement.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct CharAnimationSettings
{
    pub locomotion_in_place: bool,

    pub in_place_x: bool,
    pub in_place_y: bool,
    pub in_place_z: bool,
}

impl Default for CharAnimationSettings
{
    fn default() -> Self
    {
        Self
        {
            locomotion_in_place: true,
            in_place_x: true,
            in_place_y: false,
            in_place_z: true,
        }
    }
}

// Camera heights above the node origin (the feet).
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct CharCameraSettings
{
    pub eye_auto: bool, // derive the eye height from the head joint during auto setup
    pub eye_offset: f32, // first person: roughly where the eyes are
    pub follow_auto: bool, // orbit around the eye point, so scrolling in does not jump
    pub follow_offset: f32, // third person: what the camera orbits around
}

impl Default for CharCameraSettings
{
    fn default() -> Self
    {
        Self { eye_auto: true, eye_offset: EYE_OFFSET, follow_auto: true, follow_offset: FOLLOW_OFFSET }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct CharCollisionSettings
{
    // derive the capsule from the character bounding box during auto setup
    pub capsule_auto: bool,
    pub capsule_radius: f32,
    pub capsule_half_height: f32, // cylindrical part only, without the two caps
    pub capsule_center_offset: f32, // from the node origin (feet) up to the capsule center

    pub offset: f32, // gap the controller keeps between capsule and geometry
    pub slide: bool, // slide along walls instead of stopping dead
    pub snap_to_ground: f32, // 0 disables snapping
    pub autostep_height: f32, // 0 disables stair stepping
    pub autostep_min_width: f32,
    pub max_slope_climb_angle: f32,
    pub min_slope_slide_angle: f32,

    // The character is a shape cast, not a rigid body, so it has no mass of its own and
    // nothing it walks into ever moves. These turn the hits it already reports into impulses.
    #[serde(default = "default_push_bodies")]
    pub push_bodies: bool,
    #[serde(default = "default_push_mass")]
    pub push_mass: f32,
}

fn default_push_bodies() -> bool { true }

// roughly a person, in the same units the rest of the physics uses
fn default_push_mass() -> f32 { 70.0 }

impl Default for CharCollisionSettings
{
    fn default() -> Self
    {
        Self
        {
            capsule_auto: true,
            capsule_radius: CAPSULE_RADIUS,
            capsule_half_height: CAPSULE_HALF_HEIGHT,
            capsule_center_offset: CAPSULE_CENTER_OFFSET,

            offset: COLLISION_OFFSET,
            slide: true,
            snap_to_ground: SNAP_TO_GROUND,
            autostep_height: AUTOSTEP_HEIGHT,
            autostep_min_width: AUTOSTEP_MIN_WIDTH,
            max_slope_climb_angle: MAX_SLOPE_CLIMB_ANGLE,
            min_slope_slide_angle: MIN_SLOPE_SLIDE_ANGLE,

            push_bodies: default_push_bodies(),
            push_mass: default_push_mass(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct CharacterController
{
    base: SceneControllerBase,

    pub node_name: String,
    pub cam_name: String,

    pub fade_speed: f32,

    pub movement_speed: f32,
    pub movement_speed_fast: f32,
    pub fly_speed: f32,
    pub fly_speed_fast: f32,

    pub rotation_speed: f32,

    pub rotation_follow: bool,
    pub rotation_follow_angle_speed: f32,
    pub direction: Vector3<f32>,

    #[serde(skip, default)]
    current_target_rotation: f32, // this is just for the automatic camera rotation (rotation_follow)

    pub fall_velocity: f32,
    pub fall_stop_height: f32,
    pub rotation_offset: f32,

    // serde default so scenes saved before the capsule collision still load
    #[serde(default)]
    pub collision: CharCollisionSettings,

    #[serde(default)]
    pub animation: CharAnimationSettings,

    #[serde(default)]
    pub camera: CharCameraSettings,

    // the character must not collide with itself - rebuilt by auto_setup
    #[serde(skip, default)]
    excluded_node_ids: HashSet<u32>,

    // measure the capsule once idle poses the skin - the bind pose is a T-pose and too wide
    #[serde(skip, default)]
    capsule_setup_pending: bool,

    // collider under the feet + its last position, so moving platforms carry the character
    #[serde(skip, default)]
    ground_collider: Option<(ColliderHandle, Vector3<f32>)>,

    // one report per cause, so an intermittent hop can be traced without flooding the console
    #[serde(skip, default)]
    reported_platform_hop: bool,
    #[serde(skip, default)]
    reported_ground_loss: bool,

    #[serde(skip, default)]
    current_y_velocity: f32,

    pub gravity: f32,
    pub jump_force: f32,
    pub max_fall_speed: f32,

    pub jumps: u32,
    pub max_jumps: u32,

    pub fly_mode: bool,

    pub physics: bool,
    pub falling: bool,
    pub grounded: bool,

    pub strafe: bool,

    pub update_only_on_move: bool,

    #[serde(serialize_with = "serialize_node", deserialize_with = "deserialize_node")]
    pub node: OptionOrId<NodeItem>,

    #[serde(skip, default)]
    animation_node: Option<NodeItem>,

    #[serde(skip, default)]
    animations: AnimationComponents,

    #[serde(skip, default)]
    transformation: Option<ComponentItem>
}

impl Drop for CharacterController
{
    fn drop(&mut self)
    {
        self.cleanup();
    }
}

impl CharacterController
{
    pub fn default() -> Self
    {
        CharacterController
        {
            base: SceneControllerBase::new("Character Controller".to_string(), "🏃".to_string()),

            node_name: "".to_string(),
            cam_name: "".to_string(),

            fade_speed: FADE_SPEED,

            movement_speed: MOVEMENT_SPEED,
            movement_speed_fast: MOVEMENT_SPEED_FAST,
            fly_speed: FLY_SPEED,
            fly_speed_fast: FLY_SPEED_FAST,

            rotation_speed: ROTATION_SPEED,

            rotation_follow: true,
            rotation_follow_angle_speed: 0.075,
            direction: CHARACTER_DIRECTION,
            current_target_rotation: 0.0,

            fall_velocity: FALL_VELOCITY,
            fall_stop_height: FALL_STOP_HEIGHT,
            rotation_offset: 0.0,

            collision: CharCollisionSettings::default(),
            animation: CharAnimationSettings::default(),
            camera: CharCameraSettings::default(),

            excluded_node_ids: HashSet::new(),
            capsule_setup_pending: false,
            ground_collider: None,
            reported_platform_hop: false,
            reported_ground_loss: false,

            current_y_velocity: 0.0,
            gravity: EARTH_GRAVITY,
            jump_force: JUMP_FORCE,
            max_fall_speed: MAX_FALL_SPEED,

            jumps: 0,
            max_jumps: 2,

            fly_mode: false,

            physics: true,
            falling: true,
            grounded: false,

            strafe: false,

            update_only_on_move: false,

            node: OptionOrId::None,
            animation_node: None,

            animations: AnimationComponents::default(),

            transformation: None,
        }
    }

    pub fn auto_setup(&mut self, scene: &mut crate::state::scene::scene::Scene, character_node: &str, cam_name: &str) -> Option<String>
    {
        let node = scene.find_node_by_name(character_node);

        let cam;
        if cam_name.is_empty()
        {
            cam = scene.get_active_camera_mut();
        }
        else
        {
            cam = scene.get_camera_by_name_mut(cam_name);
        }

        if node.is_none()
        {
            console_error!("auto setup failed - node not found");
            return Some("auto setup failed - node not found".to_string());
        }

        if cam.is_none()
        {
            console_error!("auto setup failed - camera not found");
            return Some("auto setup failed - camera not found".to_string());
        }

        self.node = OptionOrId::Some(node.unwrap());
        let node_arc = self.node.clone().unwrap();
        self.node_name = node_arc.read().unwrap().name.clone();

        let cam = cam.unwrap();
        cam.node = OptionOrId::Some(node_arc.clone());
        self.cam_name = cam.name.clone();

        let mut target_rotation_controller = TargetRotationController::default();
        target_rotation_controller.data.get_mut().alpha = 0.0;
        target_rotation_controller.data.get_mut().beta = PI / 7.0;
        target_rotation_controller.data.get_mut().radius = DEFAULT_CAM_RADIUS;
        target_rotation_controller.data.get_mut().offset.y = 1.0;
        target_rotation_controller.collision_check = true;

        // the bbox center moves with the animation and would make the camera tremble
        target_rotation_controller.use_bbox_center = false;
        target_rotation_controller.data.get_mut().offset.y = self.camera.follow_offset;

        // do not include joint attached items to the check (like heads or weapons)
        target_rotation_controller.object_center_predicate = Some(Arc::new(|node: NodeItem| -> bool
        {
            let node = node.read().unwrap();
            let node_has_joint = node.find_component::<Joint>().is_some();

            if let Some(parent) = node.parent.as_ref()
            {
                let parent = parent.read().unwrap();
                let parent_has_joint = parent.find_component::<Joint>().is_some();

                if !node_has_joint && parent_has_joint
                {
                    return false;
                }
            }

            true
        }));

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

                    self.animations.blending = animation_node.find_component::<AnimationBlending>();
                }

                if self.animations.blending.is_none()
                {
                    let animation_blending = AnimationBlending::new_empty("Animation Blending");
                    animation_node.write().unwrap().add_component_front(Arc::new(RwLock::new(Box::new(animation_blending))));

                    self.animations.blending = animation_node.read().unwrap().find_component::<AnimationBlending>();
                }
            }

            let node = node_arc.read().unwrap();

            self.animations.idle = node.find_animation_by_regex("(?i)^idle");
            self.animations.walk = node.find_animation_by_include_exclude(&["walk".to_string()].to_vec(), &["strafe".to_string()].to_vec());
            self.animations.run = node.find_animation_by_include_exclude(&["run".to_string()].to_vec(), &["strafe".to_string()].to_vec());
            self.animations.jump = node.find_animation_by_regex("(?i)jump.*");
            self.animations.crouch = node.find_animation_by_regex("(?i)crouch.*");
            self.animations.roll = node.find_animation_by_regex("(?i)roll.*");
            self.animations.strafe_left_walk = node.find_animation_by_include_exclude(&["strafe".to_string(), "left".to_string(), "walk".to_string()].to_vec(), &vec![]);
            self.animations.strafe_right_walk = node.find_animation_by_include_exclude(&["strafe".to_string(), "right".to_string(), "walk".to_string()].to_vec(), &vec![]);
            self.animations.strafe_left_run = node.find_animation_by_include_exclude(&["strafe".to_string(), "left".to_string(), "run".to_string()].to_vec(), &vec![]);
            self.animations.strafe_right_run = node.find_animation_by_include_exclude(&["strafe".to_string(), "right".to_string(), "run".to_string()].to_vec(), &vec![]);
            self.animations.fall_idle = node.find_animation_by_include_exclude(&["fall".to_string()].to_vec(), &["land".to_string()].to_vec());
            self.animations.fall_landing = node.find_animation_by_include_exclude(&["fall".to_string(), "land".to_string()].to_vec(), &vec![]);
            self.animations.actions = node.find_animations_by_regex("(?i)action.*");

            // set jump animation in place - the controller drives the jump height itself
            if let Some(jump_animation) = &self.animations.jump
            {
                let root_node_arc = Self::find_root_joint_node(&self.animation_node);

                component_downcast_mut!(jump_animation, Animation);
                if let Some(root_node_arc) = root_node_arc
                {
                    jump_animation.in_place_joint_node = OptionOrId::Some(root_node_arc.clone());
                }
                else
                {
                    jump_animation.in_place_joint_node = OptionOrId::None;
                }
            }
        }

        self.apply_locomotion_in_place();

        // transformation animation
        {
            let mut node = node_arc.write().unwrap();

            if node.find_component::<Transformation>().is_none()
            {
                let component = Transformation::identity("Transformation");
                node.add_component(Arc::new(RwLock::new(Box::new(component))));
            }
            {
                let transformation = node.find_component::<Transformation>().unwrap();
                self.transformation = Some(transformation.clone());
            }
        }

        // ********** collision capsule **********
        if self.collision.capsule_auto
        {
            // usable value from the bind pose now, measured again once idle poses the skeleton
            self.setup_capsule_from_bounds();
            self.capsule_setup_pending = true;
        }

        self.refresh_excluded_nodes();

        // without colliders the shape cast finds no ground and the character falls forever
        if scene.physics.is_empty()
        {
            let colliders = scene.build_physics();
            console_log!("character controller: built {} scene colliders", colliders);
        }

        // the character is skinned, so its meshes would be re-synced every single frame
        scene.physics.exclude_nodes(&self.excluded_node_ids);

        self.start_animation(CharAnimationType::Idle, 0, AnimationMixing::Stop, 1.0, true, false, false);

        None
    }

    // Head joint, for the first person eye height. Ordered like find_root_joint_node.
    fn find_head_joint_node(animation_node: &Option<NodeItem>) -> Option<NodeItem>
    {
        const SEP: &str = "(^|[:._ -])";

        let patterns =
        [
            // an explicit eye bone is the most accurate source there is
            format!("(?i){}(left)?eye(_?l)?$", SEP),

            // the head joint sits at the base of the skull on every rig
            format!("(?i){}head$", SEP),
            format!("(?i){}head[0-9]+$", SEP),

            // mixamo end effector at the crown, and the neck as a last resort
            format!("(?i){}headtop", SEP),
            format!("(?i){}neck", SEP),
        ];

        let animation_node = animation_node.clone()?;
        let animation_node = animation_node.read().unwrap();

        patterns.iter().find_map(|pattern| animation_node.find_child_node_by_regex(pattern))
    }

    // Measures the eye height from the head joint, relative to the node origin.
    pub fn setup_eye_offset_from_head(&mut self) -> bool
    {
        let node = match self.node.as_ref() { Some(node) => node.clone(), None => return false };
        let head = match Self::find_head_joint_node(&self.animation_node) { Some(head) => head, None => return false };

        let head_y = extract_translation_from_transform(&head.read().unwrap().get_full_transform()).y;
        let node_y = extract_translation_from_transform(&node.read().unwrap().get_full_transform()).y;

        let offset = head_y - node_y;

        if offset <= 0.0
        {
            return false;
        }

        self.camera.eye_offset = offset;

        true
    }

    // Keeps the follow camera aimed at the middle of the character rather than its feet.
    pub fn apply_camera_offset(&self, scene: &mut crate::state::scene::scene::Scene)
    {
        let cam = if self.cam_name.is_empty()
        {
            scene.get_active_camera_mut()
        }
        else
        {
            let cam_name = self.cam_name.clone();
            scene.get_camera_by_name_mut(cam_name.as_str())
        };

        let Some(cam) = cam else { return; };
        let Some(controller) = cam.controller.as_mut() else { return; };
        let Some(controller) = controller.as_any_mut().downcast_mut::<TargetRotationController>() else { return; };

        let first_person = approx_zero(controller.data.get_ref().radius);
        let wanted = if first_person { self.camera.eye_offset } else { self.camera.follow_offset };

        if !approx_equal(controller.data.get_ref().offset.y, wanted)
        {
            controller.data.get_mut().offset.y = wanted;
        }
    }

    // Root joint carrying the clip root motion. Ordered, because the lookup is depth first.
    fn find_root_joint_node(animation_node: &Option<NodeItem>) -> Option<NodeItem>
    {
        const SEP: &str = "(^|[:._ -])";

        let patterns =
        [
            // explicit root bone: unreal, rigify, synty, godot
            format!("(?i){}root$", SEP),

            // 3ds max / source: Bip01 is the root, its Pelvis child must not win
            format!("(?i){}bip0*1([:._ -]?pelvis)?$", SEP),

            // hip bone: mixamo, unity humanoid, unreal, vrm, daz, character creator
            format!("(?i){}(hips?|pelvis)$", SEP),

            // rigify torso and daz abdomen
            format!("(?i){}(torso|abdomen)$", SEP),

            // last resort, matches spine, spine_01, DEF-spine, ...
            format!("(?i){}spine", SEP),
        ];

        let animation_node = animation_node.clone()?;
        let animation_node = animation_node.read().unwrap();

        patterns.iter().find_map(|pattern| animation_node.find_child_node_by_regex(pattern))
    }

    // Strips the clip root motion off walk/run/strafe - it would add to the controller movement.
    pub fn apply_locomotion_in_place(&mut self)
    {
        let root_joint = Self::find_root_joint_node(&self.animation_node);

        let axis = Vector3::new(self.animation.in_place_x, self.animation.in_place_y, self.animation.in_place_z);
        let enabled = self.animation.locomotion_in_place;

        let locomotion = vec!
        [
            self.animations.walk.clone(),
            self.animations.run.clone(),
            self.animations.strafe_left_walk.clone(),
            self.animations.strafe_right_walk.clone(),
            self.animations.strafe_left_run.clone(),
            self.animations.strafe_right_run.clone(),
        ];

        for animation in locomotion.into_iter().flatten()
        {
            component_downcast_mut!(animation, Animation);

            match (enabled, root_joint.clone())
            {
                (true, Some(root_joint)) =>
                {
                    animation.in_place_joint_node = OptionOrId::Some(root_joint);
                    animation.in_place_axis = axis;
                }
                _ =>
                {
                    animation.in_place_joint_node = OptionOrId::None;
                }
            }
        }
    }

    // Derives the capsule from the bounding box, assuming the node origin sits at the feet.
    pub fn setup_capsule_from_bounds(&mut self) -> bool
    {
        let node = match self.node.as_ref()
        {
            Some(node) => node.clone(),
            None => return false
        };

        let bounds = node.read().unwrap().get_world_bounding_info(None, true, None);

        let (min, max) = match bounds
        {
            Some(bounds) => bounds,
            None => return false
        };

        let world_transform = node.read().unwrap().get_full_transform();
        let world_pos = extract_translation_from_transform(&world_transform);

        let height = (max.y - min.y).max(0.02);

        // smaller extent = body depth; the wider one is the arm span in a bind pose
        let width = (max.x - min.x).min(max.z - min.z);

        // the two caps eat into the total height, so the radius can never reach half of it
        let radius = (width * 0.5).clamp(0.01, height * 0.5 - 0.01);

        self.collision.capsule_radius = radius;
        self.collision.capsule_half_height = (height * 0.5 - radius).max(0.0);
        self.collision.capsule_center_offset = (min.y + max.y) * 0.5 - world_pos.y;

        true
    }

    // The character and everything below it must not block its own shape cast.
    pub fn refresh_excluded_nodes(&mut self)
    {
        self.excluded_node_ids.clear();

        let node = match self.node.as_ref()
        {
            Some(node) => node.clone(),
            None => return
        };

        let child_nodes;
        {
            let node = node.read().unwrap();
            self.excluded_node_ids.insert(node.id);

            child_nodes = Scene::list_all_child_nodes(&node.nodes);
        }

        for child in child_nodes
        {
            self.excluded_node_ids.insert(child.read().unwrap().id);
        }
    }

    fn get_animation(&self, animation: CharAnimationType, index: usize) -> Option<ComponentItem>
    {
        match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animations.idle.clone(),
            CharAnimationType::Walk => self.animations.walk.clone(),
            CharAnimationType::Run => self.animations.run.clone(),
            CharAnimationType::StrafeLeftWalk => self.animations.strafe_left_walk.clone(),
            CharAnimationType::StrafeRightWalk => self.animations.strafe_right_walk.clone(),
            CharAnimationType::StrafeLeftRun => self.animations.strafe_left_run.clone(),
            CharAnimationType::StrafeRightRun => self.animations.strafe_right_run.clone(),
            CharAnimationType::Jump => self.animations.jump.clone(),
            CharAnimationType::Crouch => self.animations.crouch.clone(),
            CharAnimationType::Roll => self.animations.roll.clone(),
            CharAnimationType::Fall => self.animations.fall_idle.clone(),
            CharAnimationType::FallLanding => self.animations.fall_landing.clone(),
            CharAnimationType::Action => self.animations.actions.get(index).cloned(),
        }
    }

    fn get_animation_duration(&self, animation: CharAnimationType, index: usize) -> f32
    {
        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animations.idle.as_ref(),
            CharAnimationType::Walk => self.animations.walk.as_ref(),
            CharAnimationType::Run => self.animations.run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animations.strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animations.strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animations.strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animations.strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animations.jump.as_ref(),
            CharAnimationType::Crouch => self.animations.crouch.as_ref(),
            CharAnimationType::Roll => self.animations.roll.as_ref(),
            CharAnimationType::Fall => self.animations.fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animations.fall_landing.as_ref(),
            CharAnimationType::Action => self.animations.actions.get(index),
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
            CharAnimationType::Idle => self.animations.idle.as_ref(),
            CharAnimationType::Walk => self.animations.walk.as_ref(),
            CharAnimationType::Run => self.animations.run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animations.strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animations.strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animations.strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animations.strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animations.jump.as_ref(),
            CharAnimationType::Crouch => self.animations.crouch.as_ref(),
            CharAnimationType::Roll => self.animations.roll.as_ref(),
            CharAnimationType::Fall => self.animations.fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animations.fall_landing.as_ref(),
            CharAnimationType::Action => self.animations.actions.get(index),
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
            self.animations.idle.clone(),
            self.animations.walk.clone(),
            self.animations.run.clone(),
            self.animations.strafe_left_walk.clone(),
            self.animations.strafe_right_walk.clone(),
            self.animations.strafe_left_run.clone(),
            self.animations.strafe_right_run.clone(),
            self.animations.jump.clone(),
            self.animations.crouch.clone(),
            self.animations.roll.clone(),
            self.animations.fall_idle.clone(),
            self.animations.fall_landing.clone(),
        ];

        for action in &self.animations.actions
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
            self.animations.idle.clone(),
            self.animations.walk.clone(),
            self.animations.run.clone(),
            self.animations.strafe_left_walk.clone(),
            self.animations.strafe_right_walk.clone(),
            self.animations.strafe_left_run.clone(),
            self.animations.strafe_right_run.clone(),
            self.animations.jump.clone(),
            self.animations.crouch.clone(),
            self.animations.roll.clone(),
            self.animations.fall_idle.clone(),
            self.animations.fall_landing.clone(),
        ];

        for action in &self.animations.actions
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
            self.animations.idle.clone(),
            self.animations.walk.clone(),
            self.animations.run.clone(),
            self.animations.strafe_left_walk.clone(),
            self.animations.strafe_right_walk.clone(),
            self.animations.strafe_left_run.clone(),
            self.animations.strafe_right_run.clone(),
            self.animations.jump.clone(),
            self.animations.crouch.clone(),
            self.animations.roll.clone(),
            self.animations.fall_idle.clone(),
            self.animations.fall_landing.clone(),
        ];

        for action in &self.animations.actions
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
        if let Some(animation_jump) = &self.animations.jump
        {
            component_downcast!(animation_jump, Animation);
            return animation_jump.running() && animation_jump.animation_time() < animation_jump.to - self.fade_speed
        }

        false
    }

    fn is_rolling(&self) -> bool
    {
        if let Some(animation_roll) = &self.animations.roll
        {
            component_downcast!(animation_roll, Animation);
            return animation_roll.running() && animation_roll.animation_time() < animation_roll.to - self.fade_speed
        }

        false
    }

    fn is_landing(&self) -> bool
    {
        if let Some(animation_fall_landing) = &self.animations.fall_landing
        {
            component_downcast!(animation_fall_landing, Animation);
            return animation_fall_landing.running() && animation_fall_landing.animation_time() < animation_fall_landing.to - self.fade_speed
        }

        false
    }

    fn is_action(&self) -> bool
    {
        for animation in &self.animations.actions
        {
            component_downcast!(animation, Animation);
            if animation.running() && animation.animation_time() < animation.to - self.fade_speed
            {
                return true;
            }
        }

        false
    }

    fn get_all_animations(&self) -> Vec<ComponentItem>
    {
        let mut animation_items = vec!
        [
            self.animations.idle.clone(),
            self.animations.walk.clone(),
            self.animations.run.clone(),
            self.animations.strafe_left_walk.clone(),
            self.animations.strafe_right_walk.clone(),
            self.animations.strafe_left_run.clone(),
            self.animations.strafe_right_run.clone(),
            self.animations.jump.clone(),
            self.animations.crouch.clone(),
            self.animations.roll.clone(),
            self.animations.fall_idle.clone(),
            self.animations.fall_landing.clone(),
        ];

        for action in &self.animations.actions
        {
            animation_items.push(Some(action.clone()));
        }

        let mut animations = vec![];
        for animation in animation_items
        {
            if let Some(animation) = animation
            {
                animations.push(animation.clone());
            }
        }
        animations
    }

    fn start_animation(&mut self, animation: CharAnimationType, index: usize, mix_type: AnimationMixing, animation_speed: f32, looped: bool, reverse: bool, reset_time: bool)
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
        if let Some(animation_blending) = &self.animations.blending
        {
            component_downcast_mut!(animation_blending, AnimationBlending);
            animation_blending.to = None;
        }

        let animation_item = match animation
        {
            CharAnimationType::None => None,
            CharAnimationType::Idle => self.animations.idle.as_ref(),
            CharAnimationType::Walk => self.animations.walk.as_ref(),
            CharAnimationType::Run => self.animations.run.as_ref(),
            CharAnimationType::StrafeLeftWalk => self.animations.strafe_left_walk.as_ref(),
            CharAnimationType::StrafeRightWalk => self.animations.strafe_right_walk.as_ref(),
            CharAnimationType::StrafeLeftRun => self.animations.strafe_left_run.as_ref(),
            CharAnimationType::StrafeRightRun => self.animations.strafe_right_run.as_ref(),
            CharAnimationType::Jump => self.animations.jump.as_ref(),
            CharAnimationType::Crouch => self.animations.crouch.as_ref(),
            CharAnimationType::Roll => self.animations.roll.as_ref(),
            CharAnimationType::Fall => self.animations.fall_idle.as_ref(),
            CharAnimationType::FallLanding => self.animations.fall_landing.as_ref(),
            CharAnimationType::Action => self.animations.actions.get(index),
        };

        if mix_type == AnimationMixing::Fade && animation_item.is_some()
        {
            let animation_item = animation_item.clone().unwrap();

            if let Some(animation_blending) = &self.animations.blending
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
            animation_item.speed = animation_speed;

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

#[typetag::serde]
impl SceneController for CharacterController
{
    scene_controller_impl_default!();

    fn runs_in_mode(&self, run_mode: RunMode) -> bool
    {
        run_mode.runs_game_logic()
    }

    fn cleanup(&mut self)
    {
        // disable animation blending to prevent automatic animation restart
        let mut animation_blending_ids = vec![];
        if let Some(animation_blending) = &self.animations.blending
        {
            component_downcast_mut!(animation_blending, AnimationBlending);
            animation_blending.to = None;
            animation_blending.from = None;
            animation_blending_ids.push(animation_blending.get_base().id);
        }

        // remove all animation blending components
        if let Some(animation_node) = self.animation_node.as_ref()
        {
            animation_node.write().unwrap().remove_components_by_ids(&animation_blending_ids);
        }

        // stop all animations
        for animation in self.get_all_animations()
        {
            component_downcast_mut!(animation, Animation);
            animation.stop();
        }

        self.node = OptionOrId::None;
        self.animation_node = None;

        self.animations.idle = None;
        self.animations.walk = None;
        self.animations.run = None;
        self.animations.jump = None;
        self.animations.crouch = None;
        self.animations.roll = None;
        self.animations.strafe_left_walk = None;
        self.animations.strafe_right_walk = None;
        self.animations.strafe_left_run = None;
        self.animations.strafe_right_run = None;
        self.animations.fall_idle = None;
        self.animations.fall_landing = None;

        self.animations.actions.clear();

        self.animations.blending = None;

        self.transformation = None;
    }

    fn cleanup_node(&mut self, node: NodeItem) -> bool
    {
        if let Some(own_node) = self.node.as_ref()
        {
            if node.read().unwrap().id == own_node.read().unwrap().id
            {
                self.node = OptionOrId::None;
                return true;
            }
        }

        false
    }

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        // resolve node
        if self.node.is_ref()
        {
            let node_found = context.nodes.iter().find(|node| node.read().unwrap().uuid == self.node.id().unwrap());
            if let Some(node) = node_found
            {
                self.node = OptionOrId::Some(node.clone());
            }
            else
            {
                self.node = OptionOrId::None;
                console_error!("CharacterController: Node with id {} not found", self.node.id().unwrap());
            }
        }

        self.auto_setup(&mut context.scene, self.node_name.clone().as_str(), self.cam_name.clone().as_str());
    }

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, io: &mut InputOutput, frame_scale: f32) -> bool
    {
        if self.node.is_none()
        {
            return false;
        }

        let node = self.node.clone().unwrap();

        let mut has_change = false;

        // ********** deferred capsule measurement **********
        // the bind pose is a T-pose, so measure once the idle animation is actually posing
        if self.capsule_setup_pending && self.is_animation_running(CharAnimationType::Idle, 0)
        {
            self.capsule_setup_pending = false;

            if self.collision.capsule_auto
            {
                self.setup_capsule_from_bounds();
            }

            if self.camera.eye_auto
            {
                self.setup_eye_offset_from_head();
            }

            if self.camera.follow_auto
            {
                self.camera.follow_offset = self.camera.eye_offset;
            }

            self.apply_camera_offset(scene);
        }

        let mut movement = Vector3::<f32>::zeros();
        let mut rotation = Vector3::<f32>::zeros();

        let mut is_jumping = self.is_jumping();
        let mut is_landing = self.is_landing();
        let mut is_rolling = self.is_rolling();

        let mut is_action = self.is_action();

        // ********** fly mode **********
        if io.input_manager.keyboard.is_pressed(Key::Y)
        {
            self.fly_mode = !self.fly_mode;

            if self.fly_mode
            {
                self.start_animation(CharAnimationType::Fall, 0, AnimationMixing::Fade, 1.0, true, false, false);
            }

            if let Some(cam) = scene.get_active_camera_mut()
            {
                if let Some(controller) = cam.controller.as_mut()
                {
                    if let Some(controller) = controller.as_any_mut().downcast_mut::<TargetRotationController>()
                    {
                        controller.collision_check = !self.fly_mode;
                    }
                }
            }
        }

        // ********** first person mode **********
        let mut is_first_person = false;
        if let Some(cam) = scene.get_active_camera()
        {
            if let Some(controller) = cam.controller.as_ref()
            {
                if let Some(controller) = controller.as_any().downcast_ref::<TargetRotationController>()
                {
                    is_first_person = approx_zero(controller.data.get_ref().radius);
                }
            }
        }

        self.apply_camera_offset(scene);

        // do not show charactar in first person mode
        if let Some(node) = self.node.as_ref()
        {
            node.write().unwrap().settings.visible = !is_first_person;
        }

        // ********** forward/backward **********
        if !io.input_manager.keyboard.is_holding(Key::C) && !is_action && !is_landing
        {
            if io.input_manager.keyboard.is_holding(Key::W) && !io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling && !self.fly_mode
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, 1.0, true, false, false);
                }

                movement.z = if self.fly_mode { self.fly_speed } else { self.movement_speed };
                has_change = true;
            }
            else if io.input_manager.keyboard.is_holding(Key::S) && !io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling && !self.fly_mode
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, 1.0, true, true, false);
                }
                movement.z = if self.fly_mode { -self.fly_speed } else { -self.movement_speed };
                has_change = true;
            }
            else if io.input_manager.keyboard.is_holding(Key::W) && io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling && !self.fly_mode
                {
                    self.start_animation(CharAnimationType::Run, 0, AnimationMixing::Fade, 1.0, true, false, false);
                }

                movement.z = if self.fly_mode { self.fly_speed_fast } else { self.movement_speed_fast };
                has_change = true;
            }
            else if io.input_manager.keyboard.is_holding(Key::S) && io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                if !is_jumping && !is_rolling && !is_action && !self.falling && !self.fly_mode
                {
                    self.start_animation(CharAnimationType::Walk, 0, AnimationMixing::Fade, 1.0, true, true, false);
                }

                movement.z = if self.fly_mode { -self.fly_speed } else { -self.movement_speed };
                has_change = true;
            }
        }

        // ********** left/right **********
        if !is_landing
        {
            if io.input_manager.keyboard.is_holding(Key::A)
            {
                if (!self.strafe || io.input_manager.keyboard.is_holding(Key::W) || io.input_manager.keyboard.is_holding(Key::S)) && !is_first_person
                {
                    rotation.y = self.rotation_speed;
                }
                else
                {
                    if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                    {
                        if !self.fly_mode
                        {
                            self.start_animation(CharAnimationType::StrafeLeftRun, 0, AnimationMixing::Fade, 1.0, true, false, false);
                        }
                        movement.x = if self.fly_mode { -self.fly_speed_fast } else { -self.movement_speed_fast };
                    }
                    else
                    {
                        if !self.fly_mode
                        {
                            self.start_animation(CharAnimationType::StrafeLeftWalk, 0, AnimationMixing::Fade, 1.0, true, false, false);
                        }
                        movement.x = if self.fly_mode { -self.fly_speed } else { -self.movement_speed };
                    }
                }

                has_change = true;
            }
            else if io.input_manager.keyboard.is_holding(Key::D)
            {
                if (!self.strafe || io.input_manager.keyboard.is_holding(Key::W) || io.input_manager.keyboard.is_holding(Key::S)) && !is_first_person
                {
                    rotation.y = -self.rotation_speed;
                }
                else
                {
                    if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
                    {
                        if !self.fly_mode
                        {
                            self.start_animation(CharAnimationType::StrafeRightRun, 0, AnimationMixing::Fade, 1.0, true, false, false);
                        }
                        movement.x = if self.fly_mode { self.fly_speed_fast } else { self.movement_speed_fast };
                    }
                    else
                    {
                        if !self.fly_mode
                        {
                            self.start_animation(CharAnimationType::StrafeRightWalk, 0, AnimationMixing::Fade, 1.0, true, false, false);
                        }
                        movement.x = if self.fly_mode { self.fly_speed_fast } else { self.movement_speed };
                    }
                }

                has_change = true;
            }
        }

        // ********** up/down **********
        if io.input_manager.keyboard.is_holding(Key::C) && self.fly_mode
        {
            if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                movement.y = -self.movement_speed_fast;
            }
            else
            {
                movement.y = -self.movement_speed;
            }

            has_change = true;
        }

        if io.input_manager.keyboard.is_holding(Key::Space) && self.fly_mode
        {
            if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftShift)
            {
                movement.y = self.movement_speed_fast;
            }
            else
            {
                movement.y = self.movement_speed;
            }

            has_change = true;
        }

        // ********** jump **********
        if io.input_manager.keyboard.is_pressed_no_wait(Key::Space) && !io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) && !io.input_manager.keyboard.is_holding(Key::C) && !is_rolling && !is_action && !is_landing && !self.falling && !self.fly_mode && ((self.grounded && !is_jumping) || (self.jumps > 0 && self.jumps < self.max_jumps))
        {
            let animation_speed = self.gravity / EARTH_GRAVITY;
            self.start_animation(CharAnimationType::Jump, 0, AnimationMixing::Fade, animation_speed, false, false, true);
            self.current_y_velocity = self.jump_force;
            has_change = true;
            self.jumps += 1;
        }
        // ********** crouch **********
        else if (io.input_manager.keyboard.is_holding(Key::C) || io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl)) && approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing && !self.fly_mode
        {
            self.start_animation(CharAnimationType::Crouch, 0, AnimationMixing::Fade, 1.0, false, false, false);
            has_change = true;
        }
        // ********** roll **********
        else if io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) && !approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing && !self.falling && !self.fly_mode
        {
            if movement.z > 0.0
            {
                self.start_animation(CharAnimationType::Roll, 0, AnimationMixing::Fade, 1.0, false, false, true);
            }
            else
            {
                self.start_animation(CharAnimationType::Roll, 0, AnimationMixing::Fade, 1.0, false, true, true);
            }

            has_change = true;
        }
        // ********** action **********
        else if approx_zero_vec3(&movement) && !is_jumping && !is_rolling && !is_action && !is_landing && !self.fly_mode
        {
            if io.input_manager.keyboard.is_pressed_no_wait(Key::Key1) { self.start_animation(CharAnimationType::Action, 0, AnimationMixing::Fade, 1.0, false, false, true); has_change = true;}
            if io.input_manager.keyboard.is_pressed_no_wait(Key::Key2) { self.start_animation(CharAnimationType::Action, 1, AnimationMixing::Fade, 1.0, false, false, true); has_change = true;}
            if io.input_manager.keyboard.is_pressed_no_wait(Key::Key3) { self.start_animation(CharAnimationType::Action, 2, AnimationMixing::Fade, 1.0, false, false, true); has_change = true;}
            if io.input_manager.keyboard.is_pressed_no_wait(Key::Key4) { self.start_animation(CharAnimationType::Action, 3, AnimationMixing::Fade, 1.0, false, false, true); has_change = true;}
            if io.input_manager.keyboard.is_pressed_no_wait(Key::Key5) { self.start_animation(CharAnimationType::Action, 4, AnimationMixing::Fade, 1.0, false, false, true); has_change = true;}
        }
        // ********** stop **********
        else if io.input_manager.keyboard.is_pressed_no_wait(Key::Escape) && !self.fly_mode
        {
            self.start_animation(CharAnimationType::None, 0, AnimationMixing::Stop, 1.0, false, false, false);
            has_change = true;
        }

        // ********** refresh states **********
        is_jumping = self.is_jumping();
        is_action = self.is_action();
        is_landing = self.is_landing();
        is_rolling = self.is_rolling();

        // ********** idle **********
        if approx_zero_vec3(&movement) && !self.falling && !is_jumping && !is_rolling && !is_action && !is_landing && !io.input_manager.keyboard.is_holding_modifier(Modifier::LeftCtrl) && !io.input_manager.keyboard.is_holding(Key::C) && !self.fly_mode
        {
            self.start_animation(CharAnimationType::Idle, 0, AnimationMixing::Fade, 1.0, true, false, false);
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

        // ********** rotation **********
        if !approx_zero_vec3(&rotation)
        {
            if let Some(transformation) = &self.transformation
            {
                let rotation_frame_scale = rotation * frame_scale;

                component_downcast_mut!(transformation, Transformation);
                transformation.apply_rotation(rotation_frame_scale);
            }
        }

        // the facing direction is also needed when nothing rotated this frame
        if let Some(transformation) = &self.transformation
        {
            component_downcast!(transformation, Transformation);

            let rotation_mat = Rotation3::from_axis_angle(&Vector3::y_axis(), transformation.get_data().rotation.y + self.rotation_offset);
            self.direction = (rotation_mat * CHARACTER_DIRECTION).normalize();
        }

        // ********** movement input -> world space **********
        let movement_frame_scale = movement * frame_scale;
        let mut desired = Vector3::<f32>::zeros();

        // strafe left/right
        if !approx_zero(movement_frame_scale.x)
        {
            let rotation_strafe = Rotation3::from_axis_angle(&Vector3::y_axis(), -std::f32::consts::FRAC_PI_2);
            let strafe_dir = rotation_strafe * self.direction;

            desired += movement_frame_scale.x * strafe_dir.normalize();
        }

        // forward/backward
        if !approx_zero(movement_frame_scale.z)
        {
            desired += movement_frame_scale.z * self.direction.normalize();
        }

        // up/down in fly mode - unscaled, same as before the capsule was introduced
        desired.y += movement.y;

        // ********** gravity and collision (capsule shape cast) **********
        let run_physics = self.physics && !self.fly_mode && (!approx_zero_vec3(&movement) || !self.update_only_on_move);

        if run_physics
        {
            let delta_t = get_delta_t(frame_scale);

            // gravity only in the air - pushing into the floor stalls single frames
            if !self.grounded
            {
                self.current_y_velocity -= self.gravity * delta_t;
                self.current_y_velocity = self.current_y_velocity.clamp(-self.max_fall_speed, self.max_fall_speed);
            }

            if is_landing
            {
                self.current_y_velocity = 0.0;
            }

            // the fall/landing animations need the impact speed, which grounding resets
            let y_velocity_before = self.current_y_velocity;

            desired.y += self.current_y_velocity * delta_t;

            // read what apply_translation writes - the full transform carries the animation wobble
            let local_pos = match self.transformation.as_ref()
            {
                Some(transformation) =>
                {
                    component_downcast!(transformation, Transformation);
                    transformation.get_data().position
                }
                None => Vector3::<f32>::zeros()
            };

            // colliders are in world space, so lift a parented character out of its parent
            let mut world_pos = local_pos;
            let parent = node.read().unwrap().parent.clone();
            if let Some(parent) = parent.as_ref()
            {
                let parent_transform = parent.read().unwrap().get_full_transform();
                let world = parent_transform * Vector4::new(local_pos.x, local_pos.y, local_pos.z, 1.0);

                world_pos = Vector3::new(world.x, world.y, world.z);
            }

            // ***** moving platform *****
            // ride whatever the character was standing on last frame
            let mut platform_delta = Vector3::<f32>::zeros();
            if let Some((handle, last_translation)) = self.ground_collider
            {
                if let Some(current) = scene.physics.collider_translation(handle)
                {
                    platform_delta = current - last_translation;
                }
                else
                {
                    // the platform is gone
                    self.ground_collider = None;
                }
            }

            // A standing character should not move at all. If it does, it came either from
            // something under its feet moving, or from losing the ground for a frame.
            if self.grounded && platform_delta.y.abs() > STANDING_HOP_LIMIT && !self.reported_platform_hop
            {
                self.reported_platform_hop = true;
                console_warning!("character: carried {:.3} up or down by the collider under its feet while grounded - whatever it stands on is moving", platform_delta.y);
            }

            let world_pos = world_pos + platform_delta;

            let capsule = Capsule::new_y(self.collision.capsule_half_height, self.collision.capsule_radius);
            let capsule_pos = Pose::from_translation(Vector::new(world_pos.x, world_pos.y + self.collision.capsule_center_offset, world_pos.z));

            let mut char_controller = KinematicCharacterController::default();
            char_controller.up = Vector::Y;
            char_controller.offset = CharacterLength::Absolute(self.collision.offset);
            char_controller.slide = self.collision.slide;
            char_controller.max_slope_climb_angle = self.collision.max_slope_climb_angle;
            char_controller.min_slope_slide_angle = self.collision.min_slope_slide_angle;

            // snapping while moving upwards would pull the character back down right after take off
            char_controller.snap_to_ground = if self.collision.snap_to_ground > 0.0 && y_velocity_before <= 0.0
            {
                Some(CharacterLength::Absolute(self.collision.snap_to_ground))
            }
            else
            {
                None
            };

            char_controller.autostep = if self.collision.autostep_height > 0.0 // autostep is maybe computational intensive
            {
                Some(CharacterAutostep
                {
                    max_height: CharacterLength::Absolute(self.collision.autostep_height),
                    min_width: CharacterLength::Absolute(self.collision.autostep_min_width),
                    include_dynamic_bodies: false
                })
            }
            else
            {
                None
            };

            // out of self so the closure can borrow it while later calls need self mutably
            let excluded = std::mem::take(&mut self.excluded_node_ids);
            let predicate = |_handle: ColliderHandle, collider: &Collider| -> bool
            {
                !excluded.contains(&(collider.user_data as u32))
            };

            let filter = QueryFilter::default().predicate(&predicate);

            // every collider the character ran into on its way, so they can be pushed after
            let mut collisions = vec![];

            let movement_res =
            {
                let queries = scene.physics.query_pipeline(filter);

                char_controller.move_shape
                (
                    delta_t,
                    &queries,
                    &capsule,
                    &capsule_pos,
                    Vector::new(desired.x, desired.y, desired.z),
                    |collision| collisions.push(collision)
                )
            };

            // Turn those hits into impulses on whatever dynamic body was in the way. The
            // character itself stays unmoved by this, it has no mass to take a reaction with,
            // so push_mass only decides how hard it shoves.
            if self.collision.push_bodies && !collisions.is_empty()
            {
                let mut queries = scene.physics.query_pipeline_mut(filter);

                char_controller.solve_character_collision_impulses(delta_t, &mut queries, &capsule, self.collision.push_mass.max(0.001), &collisions);
            }

            desired = platform_delta + Vector3::new(movement_res.translation.x, movement_res.translation.y, movement_res.translation.z);

            // grounded while rising would cancel the jump on its first frame
            let landed = movement_res.grounded && y_velocity_before <= 0.0;

            if landed
            {
                if !is_jumping && !is_landing && y_velocity_before < -self.fall_velocity
                {
                    self.start_animation(CharAnimationType::FallLanding, 0, AnimationMixing::Fade, 1.0, false, false, true);
                }

                self.current_y_velocity = 0.0;
                self.falling = false;
                self.grounded = true;
                self.jumps = 0;

                // remember the collider under the feet so a moving platform can carry us
                let feet = Vector3::new
                (
                    world_pos.x + movement_res.translation.x,
                    world_pos.y + movement_res.translation.y + self.collision.capsule_center_offset,
                    world_pos.z + movement_res.translation.z
                );

                let probe_distance = self.collision.capsule_center_offset + self.collision.capsule_radius + GROUND_PROBE_MARGIN;
                self.ground_collider = scene.physics.ground_collider_below(feet, probe_distance, QueryFilter::default().predicate(&predicate));
            }
            else
            {
                if self.grounded && !is_jumping && !self.reported_ground_loss
                {
                    self.reported_ground_loss = true;
                    console_warning!("character: lost the ground for a frame while standing, y velocity {:.4} - snap_to_ground is not reaching far enough", y_velocity_before);
                }

                self.ground_collider = None;

                if !is_jumping && (y_velocity_before < -self.fall_velocity || self.falling)
                {
                    if !is_rolling
                    {
                        self.start_animation(CharAnimationType::Fall, 0, AnimationMixing::Fade, 1.0, false, false, false);
                    }

                    self.falling = true;
                }
                else
                {
                    self.falling = false;
                }

                self.grounded = false;
            }

            self.excluded_node_ids = excluded;
        }

        // ********** apply movement **********
        if !approx_zero_vec3(&desired)
        {
            // the shape cast works in world space, apply_translation expects parent space
            let mut translation = desired;

            let parent = node.read().unwrap().parent.clone();
            if let Some(parent) = parent.as_ref()
            {
                let parent_inverse = parent.read().unwrap().get_full_transform_inverse();
                let local = parent_inverse * Vector4::new(desired.x, desired.y, desired.z, 0.0);

                translation = Vector3::new(local.x, local.y, local.z);
            }

            if let Some(transformation) = &self.transformation
            {
                component_downcast_mut!(transformation, Transformation);
                transformation.apply_translation(translation);
            }

            has_change = true;
        }

        // ********** camera angle for follow mode **********
        if !approx_zero(movement.z) && !approx_zero(rotation.y) && self.rotation_follow
        {
            if let Some(cam) = scene.get_active_camera_mut()
            {
                if let Some(controller) = cam.controller.as_mut()
                {
                    if let Some(controller) = controller.as_any_mut().downcast_mut::<TargetRotationController>()
                    {
                        let (yaw, _) = yaw_pitch_from_direction(self.direction);
                        self.current_target_rotation = yaw + PI;

                        let current = controller.data.get_ref().alpha;
                        let diff = shortest_angle_dist(current, self.current_target_rotation);
                        let speed = self.rotation_follow_angle_speed * frame_scale;

                        // smooth interpolation toward target, but at least as fast as the avatar is turning
                        // so the camera never falls behind during active rotation
                        let min_delta = (rotation.y * frame_scale).abs();
                        let interp_delta = diff * speed;
                        let actual_delta = if interp_delta.abs() < min_delta && diff.abs() > min_delta
                        {
                            min_delta * diff.signum()
                        }
                        else
                        {
                            interp_delta
                        };

                        let new_alpha = current + actual_delta;
                        controller.data.get_mut().alpha = new_alpha;

                        if shortest_angle_dist(new_alpha, self.current_target_rotation).abs() < 0.05
                        {
                            controller.data.get_mut().alpha = self.current_target_rotation;
                            self.current_target_rotation = 0.0;
                        }
                    }
                }
            }
        }
        else
        {
            self.current_target_rotation = 0.0;
        }

        // ********** rotation for first person mode **********
        if let Some(cam) = scene.get_active_camera()
        {
            if let Some(controller) = cam.controller.as_ref()
            {
                if let Some(controller) = controller.as_any().downcast_ref::<TargetRotationController>()
                {
                    let controller_data = controller.data.get_ref();
                    if approx_zero(controller_data.radius)
                    {
                        if let Some(transformation) = &self.transformation
                        {
                            component_downcast_mut!(transformation, Transformation);

                            let mut rotation = transformation.get_data().rotation;
                            rotation.y = controller_data.alpha + self.rotation_offset;

                            transformation.set_rotation(rotation);

                            let rotation_mat = Rotation3::from_axis_angle(&Vector3::y_axis(), transformation.get_data().rotation.y);
                            self.direction = (rotation_mat * CHARACTER_DIRECTION).normalize();
                        }
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

        ui.horizontal(|ui|
        {
            ui.label("Camera Target Name: ");
            ui.label("ℹ").on_hover_text("leave empty for main active camera");
            ui.text_edit_singleline(&mut self.cam_name);
        });

        ui.vertical(|ui|
        {
            if ui.button("Run Auto Setup").clicked()
            {
                self.auto_setup(scene, self.node_name.clone().as_str(), self.cam_name.clone().as_str());
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
            ui.label("Fly Speed: ");
            ui.add(egui::Slider::new(&mut self.fly_speed, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Fly Speed Fast: ");
            ui.add(egui::Slider::new(&mut self.fly_speed_fast, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Rotation Speed: ");
            ui.add(egui::Slider::new(&mut self.rotation_speed, 0.0..=0.5).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Rotation Offset: ");
            ui.add(egui::Slider::new(&mut self.rotation_offset, 0.0..=PI * 2.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Fall Velocity: ");
            ui.add(egui::Slider::new(&mut self.fall_velocity, 0.0..=20.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Fall Stop Height: ");
            ui.add(egui::Slider::new(&mut self.fall_stop_height, 0.001..=1.0).fixed_decimals(3));
        });

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label("Animation");
            ui.label("ℹ").on_hover_text("how the clips interact with the movement the controller applies");
        });

        let mut in_place_changed = false;

        // without a hips/root joint the whole in place handling silently does nothing
        match Self::find_root_joint_node(&self.animation_node)
        {
            Some(joint) => { ui.label(format!("Root Joint: {}", joint.read().unwrap().name)); }
            None => { ui.colored_label(egui::Color32::from_rgb(220, 160, 60), "Root Joint: none found - in place has no effect"); }
        }

        in_place_changed |= ui.checkbox(&mut self.animation.locomotion_in_place, "Locomotion In Place").on_hover_text("strip the root motion out of walk/run/strafe - without this the clip motion adds to the controller movement and the character surges forward with every footstep").changed();

        ui.add_enabled_ui(self.animation.locomotion_in_place, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("Cancel Axis: ");
                in_place_changed |= ui.checkbox(&mut self.animation.in_place_x, "x").changed();
                in_place_changed |= ui.checkbox(&mut self.animation.in_place_y, "y").changed();
                in_place_changed |= ui.checkbox(&mut self.animation.in_place_z, "z").changed();
                ui.label("ℹ").on_hover_text("leave y off to keep the natural up/down bob of the hips");
            });
        });

        if in_place_changed
        {
            self.apply_locomotion_in_place();
        }

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label("Camera");
            ui.label("ℹ").on_hover_text("a character bounding box is rebuilt from the animated pose every frame - aiming the camera at its center makes the view tremble");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.camera.eye_auto, "Auto Eye Height");

            if ui.button("Recalculate").clicked()
            {
                self.setup_eye_offset_from_head();

                if self.camera.follow_auto
                {
                    self.camera.follow_offset = self.camera.eye_offset;
                }
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Eye Height: ");
            ui.label("ℹ").on_hover_text("first person camera height, taken from the head joint");
            ui.add(egui::Slider::new(&mut self.camera.eye_offset, 0.0..=3.0).fixed_decimals(3));
        });

        if ui.checkbox(&mut self.camera.follow_auto, "Orbit Around Eye Height").on_hover_text("keeps the third person pivot on the eye point, so scrolling into first person does not jump").changed() && self.camera.follow_auto
        {
            self.camera.follow_offset = self.camera.eye_offset;
        }

        ui.add_enabled_ui(!self.camera.follow_auto, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("Follow Height: ");
                ui.label("ℹ").on_hover_text("third person: what the camera orbits around");
                ui.add(egui::Slider::new(&mut self.camera.follow_offset, 0.0..=3.0).fixed_decimals(3));
            });
        });

        {
            let cam_name = self.cam_name.clone();
            let cam = if cam_name.is_empty() { scene.get_active_camera_mut() } else { scene.get_camera_by_name_mut(cam_name.as_str()) };

            if let Some(cam) = cam
            {
                if let Some(controller) = cam.controller.as_mut()
                {
                    if let Some(controller) = controller.as_any_mut().downcast_mut::<TargetRotationController>()
                    {
                        ui.checkbox(&mut controller.use_bbox_center, "Aim At Bounding Box Center").on_hover_text("off (recommended for characters): follow the node transform, which does not move with the animation");
                    }
                }
            }
        }

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label("Collision Capsule");
            ui.label("ℹ").on_hover_text("ground and wall collision, replaces the old downwards raycast");
        });

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.collision.capsule_auto, "Auto Size");

            if ui.button("Recalculate").clicked()
            {
                self.setup_capsule_from_bounds();
            }
        });

        ui.horizontal(|ui|
        {
            ui.label("Capsule Radius: ");
            ui.add(egui::Slider::new(&mut self.collision.capsule_radius, 0.01..=2.0).fixed_decimals(3));
        });

        ui.horizontal(|ui|
        {
            ui.label("Capsule Half Height: ");
            ui.label("ℹ").on_hover_text("cylindrical part only, without the two caps");
            ui.add(egui::Slider::new(&mut self.collision.capsule_half_height, 0.0..=2.0).fixed_decimals(3));
        });

        ui.horizontal(|ui|
        {
            ui.label("Capsule Center Offset: ");
            ui.label("ℹ").on_hover_text("from the node origin (feet) up to the capsule center");
            ui.add(egui::Slider::new(&mut self.collision.capsule_center_offset, 0.0..=4.0).fixed_decimals(3));
        });

        ui.horizontal(|ui|
        {
            ui.label("Collision Offset: ");
            ui.label("ℹ").on_hover_text("gap the controller keeps between capsule and geometry");
            ui.add(egui::Slider::new(&mut self.collision.offset, 0.001..=0.2).fixed_decimals(3));
        });

        ui.checkbox(&mut self.collision.slide, "Slide Along Walls");

        ui.horizontal(|ui|
        {
            ui.label("Snap To Ground: ");
            ui.label("ℹ").on_hover_text("keeps the character glued to the floor over small bumps - too large a value does the opposite and buries the capsule in the floor while walking, 0 disables it");
            ui.add(egui::Slider::new(&mut self.collision.snap_to_ground, 0.0..=0.1).fixed_decimals(3));
        });

        ui.horizontal(|ui|
        {
            ui.label("Autostep Height: ");
            ui.label("ℹ").on_hover_text("max stair height, 0 disables stair stepping");
            ui.add(egui::Slider::new(&mut self.collision.autostep_height, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Autostep Min Width: ");
            ui.add(egui::Slider::new(&mut self.collision.autostep_min_width, 0.0..=1.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Max Slope Climb Angle: ");
            ui.add(egui::Slider::new(&mut self.collision.max_slope_climb_angle, 0.0..=PI / 2.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Min Slope Slide Angle: ");
            ui.add(egui::Slider::new(&mut self.collision.min_slope_slide_angle, 0.0..=PI / 2.0).fixed_decimals(2));
        });

        ui.separator();

        ui.checkbox(&mut self.collision.push_bodies, "Push Dynamic Objects").on_hover_text("turns the collisions the character already reports into impulses, so it can shove things out of the way instead of just being stopped by them");

        ui.add_enabled_ui(self.collision.push_bodies, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("Push Mass: ");
                ui.add(egui::Slider::new(&mut self.collision.push_mass, 1.0..=500.0).fixed_decimals(0).suffix(" kg"));
                ui.label("ℹ").on_hover_text("the character is a shape cast, not a rigid body, so it has no mass of its own - this is only how hard it shoves. Nothing pushes back, so a heavy object gives way too, just slower");
            });
        });

        ui.separator();

        ui.label("The ground plane and the solver settings live on the scene, see Physics Settings there.");

        ui.horizontal(|ui|
        {
            ui.label(format!("Scene Colliders: {}", scene.physics.collider_amount()));
            ui.label("ℹ").on_hover_text(format!("synced last frame: {} / shape rebuilds: {}
both should be 0 while only the character moves", scene.physics.last_synced, scene.physics.last_shape_rebuilds));

            if ui.button("Rebuild").clicked()
            {
                scene.build_physics();
            }
        });

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.label("Gravity: ");
            ui.add(egui::Slider::new(&mut self.gravity, 0.0..=20.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Jump Force: ");
            ui.add(egui::Slider::new(&mut self.jump_force, 0.0..=10.0).fixed_decimals(2));
        });

        ui.horizontal(|ui|
        {
            ui.label("Max Fall Speed: ");
            ui.add(egui::Slider::new(&mut self.max_fall_speed, 0.0..=100.0).fixed_decimals(1));
        });

        ui.horizontal(|ui|
        {
            ui.label("Max Jumps: ");
            ui.add(egui::Slider::new(&mut self.max_jumps, 1..=10).fixed_decimals(0));
        });

        ui.separator();

        ui.horizontal(|ui|
        {
            ui.checkbox(&mut self.fly_mode, "Fly Mode");
        });

        ui.separator();

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

        ui.horizontal(|ui|
        {
            ui.label("Rotation Follow Angle speed: ");
            ui.add(egui::Slider::new(&mut self.rotation_follow_angle_speed, 0.0..=1.0).fixed_decimals(0));
        });
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    // builds a chain of nodes under one parent, in the given order
    fn rig(bones: &[&str]) -> Option<NodeItem>
    {
        let root = Node::new("Armature");
        let mut current = root.clone();

        for bone in bones
        {
            let node = Node::new(bone);
            Node::add_node(current.clone(), node.clone());
            current = node;
        }

        Some(root)
    }

    #[test]
    fn the_root_joint_is_found_for_the_common_rigs()
    {
        let cases: Vec<(&str, Vec<&str>, &str)> = vec!
        [
            ("mixamo",           vec!["mixamorig:Hips", "mixamorig:Spine"],            "mixamorig:Hips"),
            ("unreal mannequin", vec!["root", "pelvis", "spine_01"],                   "root"),
            ("unreal no root",   vec!["pelvis", "spine_01"],                           "pelvis"),
            ("unity humanoid",   vec!["Hips", "Spine"],                                "Hips"),
            ("vrm / vroid",      vec!["J_Bip_C_Hips", "J_Bip_C_Spine"],                "J_Bip_C_Hips"),
            ("character creator",vec!["CC_Base_Hip", "CC_Base_Waist"],                 "CC_Base_Hip"),
            ("3ds max biped",    vec!["Bip01", "Bip01 Pelvis", "Bip01 Spine"],         "Bip01"),
            ("source engine",    vec!["ValveBiped.Bip01_Pelvis", "ValveBiped.Bip01_Spine1"], "ValveBiped.Bip01_Pelvis"),
            ("rigify",           vec!["root", "DEF-spine"],                            "root"),
            ("daz genesis",      vec!["hip", "abdomenLower"],                          "hip"),
            ("spine only",       vec!["Spine", "Spine1"],                              "Spine"),
        ];

        for (rig_name, bones, expected) in cases
        {
            let animation_node = rig(&bones);
            let found = CharacterController::find_root_joint_node(&animation_node);

            let found = found.unwrap_or_else(|| panic!("{}: no root joint found in {:?}", rig_name, bones));
            let found = found.read().unwrap().name.clone();

            assert_eq!(found, expected, "{} picked the wrong bone", rig_name);
        }
    }

    #[test]
    fn the_head_joint_is_found_for_the_common_rigs()
    {
        let cases: Vec<(&str, Vec<&str>, &str)> = vec!
        [
            ("mixamo",           vec!["mixamorig:Neck", "mixamorig:Head", "mixamorig:HeadTop_End"], "mixamorig:Head"),
            ("unreal mannequin", vec!["neck_01", "head"],                     "head"),
            ("unity humanoid",   vec!["Neck", "Head"],                        "Head"),
            ("vrm / vroid",      vec!["J_Bip_C_Neck", "J_Bip_C_Head"],        "J_Bip_C_Head"),
            ("character creator",vec!["CC_Base_NeckTwist01", "CC_Base_Head"], "CC_Base_Head"),
            ("3ds max biped",    vec!["Bip01 Neck", "Bip01 Head"],            "Bip01 Head"),
            ("source engine",    vec!["ValveBiped.Bip01_Neck1", "ValveBiped.Bip01_Head1"], "ValveBiped.Bip01_Head1"),
            ("neck only",        vec!["Neck", "Shoulder_L"],                  "Neck"),
        ];

        for (rig_name, bones, expected) in cases
        {
            let animation_node = rig(&bones);
            let found = CharacterController::find_head_joint_node(&animation_node);

            let found = found.unwrap_or_else(|| panic!("{}: no head joint found in {:?}", rig_name, bones));
            let found = found.read().unwrap().name.clone();

            assert_eq!(found, expected, "{} picked the wrong bone", rig_name);
        }
    }

    #[test]
    fn unrelated_bones_are_not_mistaken_for_the_root()
    {
        // none of these should look like a hip or root bone
        let animation_node = rig(&["Armature_mesh", "Sprout", "shoulder_L", "footIK_R"]);

        assert!(CharacterController::find_root_joint_node(&animation_node).is_none());
    }
}
