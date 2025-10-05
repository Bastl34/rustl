use nalgebra::Vector3;
use serde::{Deserialize, Serialize};

use crate::{component_downcast, component_impl_default, component_impl_no_cleanup_node, component_impl_no_post_deserialization, component_impl_no_update_instance, state::{scene::{components::{animation::{Animation, AnimationLayerType}, component::{Component, ComponentBase}}, node::NodeItem}, state::InputOutput}};



#[derive(Serialize, Deserialize)]
pub struct LookAt
{
    base: ComponentBase,

    pub animation: Option<u64>,
    pub target: Vector3<f32>,

    pub offset: Vector3<f32>
}

impl LookAt
{
    pub fn new(name: &str, animation: Option<u64>, target: Vector3<f32>) -> LookAt
    {
        LookAt
        {
            base: ComponentBase::new(name.to_string(), "Look at".to_string(), "◎".to_string()),

            animation,
            target,
            offset: Vector3::<f32>::zeros()
        }
    }

    pub fn new_empty(name: &str) -> LookAt
    {
        LookAt
        {
            base: ComponentBase::new(name.to_string(), "Look at".to_string(), "◎".to_string()),

            animation: None,
            target: Vector3::<f32>::zeros(),
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

            target = self.target;
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

            self.target = target;
            self.offset = offset;
        }
    }
}