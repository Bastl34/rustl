#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use nalgebra::{Matrix4, Point3, Quaternion, UnitQuaternion, Vector3, Vector4};
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_downcast_mut, component_impl_default, component_impl_no_cleanup_node, component_impl_no_post_deserialization, component_impl_no_update_instance, console_debug, console_warning, helper::{math::{approx_zero_vec3, look_at_rotation}, option_or_id::OptionOrId}, state::{scene::{components::{animation::{Animation, AnimationLayerType}, component::{Component, ComponentBase}}, node::{Node, NodeItem}}, state::InputOutput}};
use crate::state::scene::exporter::serialization_helper;

#[derive(Serialize, Deserialize)]
pub struct LookAt
{
    base: ComponentBase,

    #[serde(serialize_with = "serialization_helper::serialize_node", deserialize_with = "serialization_helper::deserialize_node")]
    pub target_joint_item: OptionOrId<NodeItem>,

    #[serde(skip, default)]
    pub animation: Option<u64>,

    pub parent_transform_inv: Option<Matrix4::<f32>>,

    pub target_dir: Vector3<f32>,

    pub offset: Vector3<f32>
}

impl LookAt
{
    pub fn new(name: &str, target_item: NodeItem, target: Vector3<f32>) -> LookAt
    {
        LookAt
        {
            base: ComponentBase::new(name.to_string(), "Look at".to_string(), "◎".to_string()),

            target_joint_item: OptionOrId::Some(target_item),

            animation: None,

            parent_transform_inv: None,

            target_dir: target,
            offset: Vector3::<f32>::zeros()
        }
    }

    pub fn new_empty(name: &str) -> LookAt
    {
        LookAt
        {
            base: ComponentBase::new(name.to_string(), "Look at".to_string(), "◎".to_string()),

            target_joint_item: OptionOrId::None,

            animation: None,

            parent_transform_inv: None,

            target_dir: Vector3::<f32>::zeros(),
            offset: Vector3::<f32>::zeros()
        }
    }
}


#[typetag::serde]
impl Component for LookAt
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
        }
    }

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        None
    }

    fn update(&mut self, node: NodeItem, _io: &mut InputOutput, time: u128, frame_scale: f32, _frame: u64)
    {
        if !self.base.is_enabled
        {
            return;
        }

        if self.target_joint_item.is_none()
        {
            return;
        }

        // set up if needed
        if self.animation.is_none()
        {
            let mut animation = Animation::new_joint_transform_quat
            (
                "Aim Animation",
                self.target_joint_item.clone().unwrap(),
                None,
                None,
                None,
            );

            let animation_id = animation.id();

            animation.layer_type = AnimationLayerType::AdditiveComponentAbsolute;
            animation.get_base_mut().export = false; // should not be exported (becase its setup automatically)
            node.write().unwrap().add_component(Arc::new(RwLock::new(Box::new(animation))));

            // get transform between root joint and root node (because AdditiveComponentAbsolute just takes "full" joint transform into account - and nothing inbetween root and joint root)
            let parent_transform = Node::get_transform_between_root_joint_and_root_node(self.target_joint_item.clone().unwrap());
            let parent_inv = parent_transform.try_inverse().unwrap_or(Matrix4::<f32>::identity());

            self.parent_transform_inv = Some(parent_inv);

            self.animation = Some(animation_id);
        }

        let node = node.read().unwrap();

        if let Some(animation) = self.animation
        {
            if let Some(animation) = node.find_component_by_id(animation)
            {
                let mut look_rot: Option<Quaternion<f32>> = None;

                {
                    component_downcast!(animation, Animation);

                    if animation.layer_type != AnimationLayerType::AdditiveComponentAbsolute
                    {
                        console_warning!("look at: animation is not of layer type AdditiveComponentAbsolute - not supported");
                        return;
                    }

                    let channel = animation.channels.first().clone();

                    if let Some(channel) = channel
                    {
                        if let Some(target_node) = channel.target.as_ref()
                        {
                            let full_joint_transform = target_node.read().unwrap().get_full_joint_transform(None, false);
                            let joint_pos_world = full_joint_transform.transform_point(&Point3::origin());

                            // offset
                            let mut dir_world = self.target_dir;
                            if !approx_zero_vec3(&self.offset)
                            {
                                let offset_quat = UnitQuaternion::from_euler_angles
                                (
                                    self.offset.x,
                                    self.offset.y,
                                    self.offset.z,
                                );
                                dir_world = (offset_quat * dir_world).normalize();
                            }

                            let dir_world = (dir_world - joint_pos_world.coords).normalize();

                            // convert to local space ot the joint
                            let dir_local = match &self.parent_transform_inv
                            {
                                Some(inv) => (inv.transform_vector(&dir_world)).normalize(),
                                None => dir_world,
                            };

                            let up_local = Vector3::y_axis().into_inner();

                            // calc look at rotation
                            look_rot = Some(*look_at_rotation(dir_local, up_local));
                        }
                    }
                }


                if look_rot.is_none()
                {
                    return;
                }

                let look_rot = look_rot.unwrap();

                component_downcast_mut!(animation, Animation);
                animation.start();

                let channel = animation.channels.first_mut();

                if let Some(channel) = channel
                {
                    // update
                    channel.transform_rotation.clear();
                    channel.transform_rotation.push(Vector4::new(look_rot.i, look_rot.j, look_rot.k, look_rot.w));
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut animation;
        let mut animation_name;

        let mut target;
        let mut offset;

        {
            animation = self.animation.unwrap_or(0);
            animation_name = "".to_string();

            target = self.target_dir;
            offset = self.offset;
        }

        let mut animations: Vec<(u64, String)> = vec![];

        if let Some(node) = node
        {
            let node = node.read().unwrap();
            let animation_components = node.find_components::<Animation>();

            for animation_item in animation_components
            {
                component_downcast!(animation_item, Animation);

                if animation_item.layer_type != AnimationLayerType::AdditiveComponentAbsolute
                {
                    continue;
                }

                animations.push((animation_item.get_base().id, animation_item.get_base().name.clone()));

                if animation == animation_item.get_base().id
                {
                    animation_name = animation_item.get_base().name.clone();
                }
            }
        }

        ui.horizontal(|ui|
        {
            ui.label("Animation: ");
            egui::ComboBox::from_id_salt(ui.make_persistent_id("animation")).selected_text(animation_name.clone()).show_ui(ui, |ui|
            {
                changed = ui.selectable_value(&mut animation, 0, "").changed() || changed;
                for animation_item in &animations
                {
                    changed = ui.selectable_value(&mut animation, animation_item.0, animation_item.1.clone()).changed() || changed;
                }
            });
        });

        ui.horizontal(|ui|
        {
            ui.label("Target: ");
            let changed_x = ui.add(egui::DragValue::new(&mut target.x).range(-1.0..=1.0).speed(0.1).prefix("x: ")).changed();
            let changed_y = ui.add(egui::DragValue::new(&mut target.y).range(-1.0..=1.0).speed(0.1).prefix("y: ")).changed();
            let changed_z = ui.add(egui::DragValue::new(&mut target.z).range(-1.0..=1.0).speed(0.1).prefix("z: ")).changed();

            if changed_x { target.y = target.x; target.z = target.x; }
            if changed_y { target.x = target.y; target.z = target.y; }
            if changed_z { target.x = target.z; target.y = target.z; }

            changed = changed_x || changed_y || changed_z || changed;
        });

        ui.horizontal(|ui|
        {
            ui.label("Offset: ");
            let changed_x: bool = ui.add(egui::DragValue::new(&mut offset.x).range(-1.0..=1.0).speed(0.1).prefix("x: ")).changed();
            let changed_y = ui.add(egui::DragValue::new(&mut offset.y).range(-1.0..=1.0).speed(0.1).prefix("y: ")).changed();
            let changed_z = ui.add(egui::DragValue::new(&mut offset.z).range(-1.0..=1.0).speed(0.1).prefix("z: ")).changed();

            if changed_x { offset.y = offset.x; offset.z = offset.x; }
            if changed_y { offset.x = offset.y; offset.z = offset.y; }
            if changed_z { offset.x = offset.z; offset.y = offset.z; }

            changed = changed_x || changed_y || changed_z || changed;
        });

        if changed
        {
            if animation > 0
            {
                self.animation = Some(animation);
            }
            else
            {
                self.animation = None
            }

            self.target_dir = target;
            self.offset = offset;
        }
    }
}