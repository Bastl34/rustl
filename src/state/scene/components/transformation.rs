use std::any::Any;

use nalgebra::{Vector3, Matrix4, Rotation3, Matrix3, Vector4, UnitQuaternion, Quaternion};

use crate::{component_impl_default, helper::{change_tracker::ChangeTracker, math::{self, approx_zero_vec4}}, state::{scene::node::NodeItem}, component_impl_no_update};

use super::component::{Component, ComponentBase};

pub struct TransformationData
{
    pub parent_inheritance: bool,
    pub transform_vectors: bool, // if disabled - only trans matrix is used (position, rotation, scale vectors are ignored)

    pub position: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub rotation_quat: Option<Vector4<f32>>,
    pub scale: Vector3<f32>,

    trans: Matrix4<f32>,
    tran_inverse: Matrix4<f32>
}

pub struct Transformation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationData>
}

impl Transformation
{
    pub fn new(id: u64, name: &str, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: true,

            position,
            rotation,
            rotation_quat: None,
            scale,

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(id, name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data)
        };
        transform.calc_transform();

        transform
    }

    pub fn new_transformation_only(id: u64, name: &str, trans: Matrix4::<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: false,

            position: Vector3::<f32>::zeros(),
            rotation: Vector3::<f32>::zeros(),
            rotation_quat: None,
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            trans: trans,
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(id, name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data)
        };
        transform.calc_transform();

        transform
    }

    pub fn identity(id: u64, name: &str) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: true,

            position: Vector3::<f32>::new(0.0, 0.0, 0.0),
            rotation: Vector3::<f32>::new(0.0, 0.0, 0.0),
            rotation_quat: None,
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(id, name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data)
        };
        transform.calc_transform();

        transform
    }

    pub fn get_data(&self) -> &TransformationData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<TransformationData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<TransformationData>
    {
        &mut self.data
    }

    pub fn has_parent_inheritance(&self) -> bool
    {
        self.data.get_ref().parent_inheritance
    }

    pub fn calc_transform(&mut self)
    {
        let data = self.data.get_mut();

        if data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(data.position.x, data.position.y, data.position.z).to_homogeneous();

            let scale = Matrix4::new_nonuniform_scaling(&data.scale);

            let rotation_x  = Rotation3::from_euler_angles(data.rotation.x, 0.0, 0.0).to_homogeneous();
            let rotation_y  = Rotation3::from_euler_angles(0.0, data.rotation.y, 0.0).to_homogeneous();
            let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, data.rotation.z).to_homogeneous();

            let mut rotation = rotation_z;
            rotation = rotation * rotation_y;
            rotation = rotation * rotation_x;

            let mut rotation_quat: Option<Matrix4<f32>> = None;
            if let Some(data_rotation_quat) = data.rotation_quat.as_ref()
            {
                let quaternion = UnitQuaternion::new_normalize
                (
                    Quaternion::new
                    (
                        data_rotation_quat.w,
                        data_rotation_quat.x,
                        data_rotation_quat.y,
                        data_rotation_quat.z,
                    )
                );

                let rotation: Rotation3<f32> = quaternion.into();
                let rotation = rotation.to_homogeneous();

                rotation_quat = Some(rotation);
            }

            let mut trans = Matrix4::<f32>::identity();
            trans = trans * translation;
            trans = trans * rotation;

            if let Some(rotation_quat) = rotation_quat
            {
                trans = trans * rotation_quat;
            }

            trans = trans * scale;

            data.trans = trans;
        }

        data.tran_inverse = data.trans.try_inverse().unwrap();
    }

    pub fn get_transform(&self) -> &Matrix4::<f32>
    {
        &self.data.get_ref().trans
    }

    pub fn get_transform_inverse(&self) -> &Matrix4::<f32>
    {
        &self.data.get_ref().tran_inverse
    }


    pub fn apply_transformation(&mut self, translation: Option<Vector3<f32>>, scale: Option<Vector3<f32>>, rotation: Option<Vector3<f32>>)
    {
        if translation.is_none() && scale.is_none() && rotation.is_none()
        {
            return;
        }

        let data = self.data.get_mut();

        if let Some(translation) = translation
        {
            data.position += translation;
        }

        if let Some(scale) = scale
        {
            data.scale.x *= scale.x;
            data.scale.y *= scale.y;
            data.scale.z *= scale.z;

            // if its zero -> inverse matrix can not be calculated
            if math::approx_zero(data.scale.x) { data.scale.x = 0.00000001; }
            if math::approx_zero(data.scale.y) { data.scale.y = 0.00000001; }
            if math::approx_zero(data.scale.z) { data.scale.z = 0.00000001; }
        }

        if let Some(rotation) = rotation
        {
            data.rotation += rotation;
        }

        if !data.transform_vectors
        {
            let mut translation_mat = Matrix4::<f32>::identity();
            let mut rotation_mat = Matrix4::<f32>::identity();
            let mut scale_mat = Matrix4::<f32>::identity();

            if let Some(translation) = translation
            {
                translation_mat = nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
            }


            if let Some(scale) = scale
            {
                scale_mat = Matrix4::new_nonuniform_scaling(&scale);
            }

            if let Some(rotation) = rotation
            {
                let rotation_x  = Rotation3::from_euler_angles(rotation.x, 0.0, 0.0).to_homogeneous();
                let rotation_y  = Rotation3::from_euler_angles(0.0, rotation.y, 0.0).to_homogeneous();
                let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, rotation.z).to_homogeneous();

                let mut rotation = rotation_z;
                rotation = rotation * rotation_y;
                rotation = rotation * rotation_x;

                rotation_mat = rotation;
            }

            let mut trans = Matrix4::<f32>::identity();
            trans = trans * translation_mat;
            trans = trans * rotation_mat;
            trans = trans * scale_mat;

            data.trans = data.trans * trans;
        }

        self.calc_transform();
    }

    pub fn apply_translation(&mut self, translation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.position += translation;

        if !data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
            data.trans = data.trans * translation;
        }

        self.calc_transform();
    }

    pub fn set_translation(&mut self, translation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.position = translation;

        if !data.transform_vectors
        {
            let translation = nalgebra::Isometry3::translation(translation.x, translation.y, translation.z).to_homogeneous();
            data.trans = translation;
        }

        self.calc_transform();
    }

    pub fn apply_scale(&mut self, scale: Vector3<f32>, multiply: bool)
    {
        let data = self.data.get_mut();

        // the default is to multiply a new scale value - but sometimes its nessesary to add the value instead of multiplying
        if multiply
        {
            data.scale.x *= scale.x;
            data.scale.y *= scale.y;
            data.scale.z *= scale.z;
        }
        else
        {
            data.scale.x += scale.x;
            data.scale.y += scale.y;
            data.scale.z += scale.z;
        }

        // if its zero -> inverse matrix can not be calculated
        if math::approx_zero(data.scale.x) { data.scale.x = 0.00000001; }
        if math::approx_zero(data.scale.y) { data.scale.y = 0.00000001; }
        if math::approx_zero(data.scale.z) { data.scale.z = 0.00000001; }

        if !data.transform_vectors
        {
            let scale = Matrix4::new_nonuniform_scaling(&scale);

            if multiply
            {
                data.trans = data.trans * scale;
            }
            else
            {
                data.trans = data.trans + scale;
            }

            // if its zero -> inverse matrix can not be calculated
            if math::approx_zero(data.trans[(0, 0)]) { data.trans[(0, 0)] = 0.00000001; }
            if math::approx_zero(data.trans[(1, 1)]) { data.trans[(1, 1)] = 0.00000001; }
            if math::approx_zero(data.trans[(2, 2)]) { data.trans[(2, 2)] = 0.00000001; }
        }

        self.calc_transform();
    }

    pub fn apply_rotation(&mut self, rotation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.rotation += rotation;

        if !data.transform_vectors
        {
            let rotation_x  = Rotation3::from_euler_angles(rotation.x, 0.0, 0.0).to_homogeneous();
            let rotation_y  = Rotation3::from_euler_angles(0.0, rotation.y, 0.0).to_homogeneous();
            let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, rotation.z).to_homogeneous();

            let mut rotation = rotation_z;
            rotation = rotation * rotation_y;
            rotation = rotation * rotation_x;

            data.trans = data.trans * rotation;
        }

        self.calc_transform();
    }

    pub fn apply_rotation_quaternion(&mut self, rotation: Vector4<f32>)
    {
        let data = self.data.get_mut();

        if data.rotation_quat.is_none()
        {
            data.rotation_quat = Some(rotation)
        }
        else
        {
            let data_rot_quat = data.rotation_quat.as_mut().unwrap();
            data_rot_quat.x += rotation.x;
            data_rot_quat.y += rotation.y;
            data_rot_quat.z += rotation.z;
            data_rot_quat.w += rotation.w;
        }

        if approx_zero_vec4(data.rotation_quat.as_ref().unwrap())
        {
            // quaterion = 0 is not supported / working -> otherwise a inverse transform can not be created
            data.rotation_quat.as_mut().unwrap().w = 0.00000001;
        }

        if !data.transform_vectors
        {
            if let Some(data_rotation_quat) = data.rotation_quat.as_ref()
            {
                let quaternion = UnitQuaternion::new_normalize
                (
                    Quaternion::new
                    (
                        data_rotation_quat.w,
                        data_rotation_quat.x,
                        data_rotation_quat.y,
                        data_rotation_quat.z,
                    )
                );

                let rotation: Rotation3<f32> = quaternion.into();
                let rotation = rotation.to_homogeneous();

                data.trans = data.trans * rotation;
            }
        }

        self.calc_transform();
    }
}

impl Component for Transformation
{
    component_impl_default!();
    component_impl_no_update!();

    fn instantiable(&self) -> bool
    {
        true
    }

    fn set_enabled(&mut self, state: bool)
    {
        if self.base.is_enabled != state
        {
            self.base.is_enabled = state;

            // force update
            self.data.force_change();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui)
    {
        let mut changed = false;

        let mut pos;
        let mut rot;
        let mut rot_quat;
        let mut scale;
        let mut inheritance;

        {
            let data = self.get_data();

            pos = data.position;
            rot = data.rotation;
            rot_quat = data.rotation_quat;
            scale = data.scale;
            inheritance = data.parent_inheritance;

            ui.vertical(|ui|
            {
                changed = ui.checkbox(&mut inheritance, "parent transformation inheritance").changed() || changed;

                ui.horizontal(|ui|
                {
                    ui.label("Position: ");
                    changed = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed() || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Rotation\n(Euler): ");
                    changed = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed() || changed;
                });

                if let Some(rot_quat) = rot_quat.as_mut()
                {
                    ui.horizontal(|ui|
                    {
                        ui.label("Rotation\n(Quaternion): ");
                        changed = ui.add(egui::DragValue::new(&mut rot_quat.x).speed(0.1).prefix("x: ")).changed() || changed;
                        changed = ui.add(egui::DragValue::new(&mut rot_quat.y).speed(0.1).prefix("y: ")).changed() || changed;
                        changed = ui.add(egui::DragValue::new(&mut rot_quat.z).speed(0.1).prefix("z: ")).changed() || changed;
                        changed = ui.add(egui::DragValue::new(&mut rot_quat.w).speed(0.1).prefix("w: ")).changed() || changed;

                        if changed && approx_zero_vec4(rot_quat)
                        {
                            // quaterion = 0 is not supported / working -> otherwise a inverse transform can not be created
                            rot_quat.w = 0.00000001;
                        }
                    });
                }

                ui.horizontal(|ui|
                {
                    ui.label("Scale: ");
                    changed = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed() || changed;
                    changed = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed() || changed;

                    // scale = 0 is not supported / working -> otherwise a inverse transform can not be created
                    if scale.x == 0.0 { scale.x = 0.00000001; }
                    if scale.y == 0.0 { scale.y = 0.00000001; }
                    if scale.z == 0.0 { scale.z = 0.00000001; }
                });

                if rot_quat.is_none()
                {
                    if ui.button("add Quaternion Rotation").clicked()
                    {
                        rot_quat = Some(Vector4::<f32>::new(0.0, 0.0, 0.0, 1.0));
                        changed = true;
                    }
                }
            });
        }

        if changed
        {
            let data = self.get_data_mut();
            data.get_mut().position = pos;
            data.get_mut().rotation = rot;
            data.get_mut().rotation_quat = rot_quat;
            data.get_mut().scale = scale;
            data.get_mut().parent_inheritance = inheritance;
            self.calc_transform();
        }
    }
}