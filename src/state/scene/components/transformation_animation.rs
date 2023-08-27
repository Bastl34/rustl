use std::any::Any;

use egui::Color32;
use nalgebra::Vector3;

use crate::{helper::{change_tracker::ChangeTracker, self}, component_impl_default, state::scene::node::NodeItem, component_downcast, component_downcast_mut};

use super::{component::{ComponentBase, Component, ComponentItem}, transformation::Transformation};

pub struct TransformationAnimationData
{
    pub translation: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scale: Vector3<f32>
}

pub struct TransformationAnimation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationAnimationData>
}

impl TransformationAnimation
{
    pub fn new(id: u64, translation: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> TransformationAnimation
    {
        let data = TransformationAnimationData
        {
            translation,
            rotation,
            scale
        };

        let mut transform_animation = TransformationAnimation
        {
            base: ComponentBase::new(id, "Default".to_string(), "Transformation Animation".to_string(), "🏃".to_string()),
            data: ChangeTracker::new(data)
        };

        transform_animation
    }

    pub fn get_data(&self) -> &TransformationAnimationData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<TransformationAnimationData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<TransformationAnimationData>
    {
        &mut self.data
    }

    fn _update(&mut self, transform_component: Option<ComponentItem>, frame_scale: f32)
    {
        if let Some(transform_component) = transform_component
        {
            component_downcast_mut!(transform_component, Transformation);

            let data = self.get_data();
            let mut translation = None;
            let mut rotation = None;
            let mut scale = None;

            if !helper::math::approx_zero(data.translation.x) || !helper::math::approx_zero(data.translation.y) || !helper::math::approx_zero(data.translation.z)
            {
                translation = Some(Vector3::<f32>::new(data.translation.x * frame_scale, data.translation.y * frame_scale, data.translation.z * frame_scale));
            }

            if !helper::math::approx_zero(data.rotation.x) || !helper::math::approx_zero(data.rotation.y) || !helper::math::approx_zero(data.rotation.z)
            {
                rotation = Some(Vector3::<f32>::new(data.rotation.x * frame_scale, data.rotation.y * frame_scale, data.rotation.z * frame_scale));
            }

            if !helper::math::approx_zero(data.scale.x) || !helper::math::approx_zero(data.scale.y) || !helper::math::approx_zero(data.scale.z)
            {
                scale = Some(Vector3::<f32>::new(data.scale.x * frame_scale, data.scale.y * frame_scale, data.scale.z * frame_scale));
            }

            transform_component.apply_transformation(translation, None, rotation);

            if let Some(scale) = scale
            {
                transform_component.apply_scale(scale, false);
            }
        }
    }
}

impl Component for TransformationAnimation
{
    component_impl_default!();

    fn instantiable(&self) -> bool
    {
        true
    }

    fn update(&mut self, node: NodeItem, frame_scale: f32)
    {
        let node = node.write().unwrap();

        self._update(node.find_component::<Transformation>(), frame_scale);
    }

    fn update_instance(&mut self, node: NodeItem, instance: &crate::state::scene::node::InstanceItemChangeTracker, frame_scale: f32)
    {
        let instance = instance.borrow();
        let instance = instance.get_ref();

        self._update(instance.find_component::<Transformation>(), frame_scale);
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let mut changed = false;

        let mut trans;
        let mut rot;
        let mut scale;
        {
            let data = self.get_data();

            trans = data.translation;
            rot = data.rotation;
            scale = data.scale;

            /*
            let bg_color = Color32::from_white_alpha(5);

            // Rahmengröße
            let frame_size = ui.available_size();

            // Zeichne den Hintergrund des Rahmens
            let painter = ui.painter();
            painter.rect_filled(ui.max_rect(), 8.0, bg_color);

            // Definiere den Inhalt des Rahmens
            ui.allocate_ui(frame_size, |ui| {
                ui.label(egui::RichText::new("The changes are applies on the Transform Component (multiplied by frame_scale for each frame). If there is no Transform Component. Nothing is happening."));
            });
             */

            ui.vertical(|ui|
            {
                ui.horizontal(|ui|
                {
                    ui.label("Translation: ");
                    changed = ui.add(egui::DragValue::new(&mut trans.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Rotation: ");
                    changed = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Scale: ");
                    changed = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
            });
        }

        if changed
        {
            let data = self.get_data_mut();
            let data = data.get_mut();
            data.translation = trans;
            data.rotation = rot;
            data.scale = scale;
        }
    }
}