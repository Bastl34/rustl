use std::any::Any;

use nalgebra::Vector3;

use crate::{helper::{change_tracker::ChangeTracker, self}, component_impl_default, state::scene::node::NodeItem, component_downcast, component_downcast_mut};

use super::{component::{ComponentBase, Component}, transformation::Transformation};

pub struct TransformationAnimationData
{
    pub translation: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub scale: Vector3<f32>,
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
            scale,
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
}

impl Component for TransformationAnimation
{
    component_impl_default!();

    fn update(&mut self, node: NodeItem, frame_scale: f32)
    {
        let node = node.write().unwrap();
        if let Some(transform_component) = node.find_component::<Transformation>()
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

            if !helper::math::approx_equal(data.scale.x, 1.0) || !helper::math::approx_equal(data.scale.y, 1.0) || !helper::math::approx_equal(data.scale.z, 1.0)
            {
                let mut scale_vec = Vector3::<f32>::new(data.scale.x * frame_scale, data.scale.y * frame_scale, data.scale.z * frame_scale);

                // if its zero -> inverse matrix can not be calculated
                if helper::math::approx_zero(scale_vec.x) { scale_vec.x = 0.000001; }
                if helper::math::approx_zero(scale_vec.y) { scale_vec.y = 0.000001; }
                if helper::math::approx_zero(scale_vec.z) { scale_vec.z = 0.000001; }

                scale = Some(scale_vec);
            }

            transform_component.apply_transformation(translation, scale, rotation);
        }
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

            ui.label(egui::RichText::new("The changes are applies on the Transform Component. If there is no Transform Component. Nothing is happening."));

            ui.vertical(|ui|
            {
                ui.horizontal(|ui|
                {
                    ui.label("translation");
                    changed = ui.add(egui::DragValue::new(&mut trans.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut trans.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("rotation");
                    changed = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("scale");
                    changed = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed() || changed;
                });

            });
        }

        if changed
        {
            let data = self.get_data_mut();
            data.get_mut().translation = trans;
            data.get_mut().rotation = rot;
            data.get_mut().scale = scale;
        }
    }
}