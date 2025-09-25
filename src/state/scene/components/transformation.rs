#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use egui::RichText;
use nalgebra::{Vector3, Matrix4, Rotation3, Vector4, UnitQuaternion, Quaternion};
use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, component_impl_no_update, helper::{change_tracker::ChangeTracker, math::{self, approx_zero_vec4}}, state::scene::node::NodeItem};

use super::component::{Component, ComponentBase};

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct TransformationData
{
    pub parent_inheritance: bool,
    pub transform_vectors: bool, // if disabled - only trans matrix is used (position, rotation, scale vectors are ignored)

    pub position: Vector3<f32>,
    pub rotation: Vector3<f32>,
    pub rotation_quat: Option<Vector4<f32>>,
    pub scale: Vector3<f32>,

    pub animation_weight: f32,
    pub animation_update_frame: Option<u64>,

     // only supported with transform_vectors
    pub animation_position: Option<Vector3<f32>>,
    pub animation_rotation_quat: Option<Vector4<f32>>,
    pub animation_scale: Option<Vector3<f32>>,

    trans: Matrix4<f32>,

    #[serde(skip, default)]
    tran_inverse: Matrix4<f32>
}

#[derive(Serialize, Deserialize)]
pub struct Transformation
{
    base: ComponentBase,
    data: ChangeTracker<TransformationData>,

    ui_lock_translation: bool,
    ui_lock_rotation: bool,
    ui_lock_rotation_quat: bool,
    ui_lock_scale: bool,
}

impl Transformation
{
    pub fn new(name: &str, position: Vector3<f32>, rotation: Vector3<f32>, scale: Vector3<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: true,

            position,
            rotation,
            rotation_quat: None,
            scale,

            animation_weight: 0.0,
            animation_update_frame: None,

            // animation transformation is overwriting position/rotaion/rotation_quat/scale
            animation_position: None,
            animation_rotation_quat: None,
            animation_scale: None,

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data),

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
        };
        transform.calc_transform();

        transform
    }

    pub fn new_transformation_only(name: &str, trans: Matrix4::<f32>) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: false,

            position: Vector3::<f32>::zeros(),
            rotation: Vector3::<f32>::zeros(),
            rotation_quat: None,
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            animation_weight: 0.0,
            animation_update_frame: None,

            animation_position: None,
            animation_rotation_quat: None,
            animation_scale: None,

            trans: trans,
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data),

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
        };
        transform.calc_transform();

        transform
    }

    pub fn identity(name: &str) -> Transformation
    {
        let data = TransformationData
        {
            parent_inheritance: true,
            transform_vectors: true,

            position: Vector3::<f32>::new(0.0, 0.0, 0.0),
            rotation: Vector3::<f32>::new(0.0, 0.0, 0.0),
            rotation_quat: None,
            scale: Vector3::<f32>::new(1.0, 1.0, 1.0),

            animation_weight: 0.0,
            animation_update_frame: None,

            animation_position: None,
            animation_rotation_quat: None,
            animation_scale: None,

            trans: Matrix4::<f32>::identity(),
            tran_inverse: Matrix4::<f32>::identity()
        };

        let mut transform = Transformation
        {
            base: ComponentBase::new(name.to_string(), "Transformation".to_string(), "📌".to_string()),
            data: ChangeTracker::new(data),

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
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
            // ********** translation **********
            let translation;

            if let Some(animation_position) = &data.animation_position
            {
                //translation = translation * nalgebra::Isometry3::translation(animation_position.x, animation_position.y, animation_position.z).to_homogeneous();
                translation = nalgebra::Isometry3::translation(animation_position.x, animation_position.y, animation_position.z).to_homogeneous();
            }
            else
            {
                translation = nalgebra::Isometry3::translation(data.position.x, data.position.y, data.position.z).to_homogeneous();
            }

            // ********** scale **********
            let scale;

            if let Some(animation_scale) = &data.animation_scale
            {
                //scale = scale * Matrix4::new_nonuniform_scaling(&animation_scale);
                scale = Matrix4::new_nonuniform_scaling(&animation_scale);
            }
            else
            {
                scale = Matrix4::new_nonuniform_scaling(&data.scale);
            }

            // ********** rotation **********
            let mut rotation: Matrix4<f32>;
            if let Some(animation_rotation_quat) = &data.animation_rotation_quat
            {
                let quaternion = UnitQuaternion::new_normalize
                (
                    Quaternion::new
                    (
                        animation_rotation_quat.w,
                        animation_rotation_quat.x,
                        animation_rotation_quat.y,
                        animation_rotation_quat.z,
                    )
                );

                let rotation_quat: Rotation3<f32> = quaternion.into();
                let rotation_quat = rotation_quat.to_homogeneous();

                rotation = rotation_quat;
            }
            else
            {
                let rotation_x  = Rotation3::from_euler_angles(data.rotation.x, 0.0, 0.0).to_homogeneous();
                let rotation_y  = Rotation3::from_euler_angles(0.0, data.rotation.y, 0.0).to_homogeneous();
                let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, data.rotation.z).to_homogeneous();

                // yaw, pitch, roll
                rotation = rotation_z;
                rotation = rotation * rotation_y;
                rotation = rotation * rotation_x;

                // ********** quaternion rotation **********
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

                    let rotation_quat: Rotation3<f32> = quaternion.into();
                    let rotation_quat = rotation_quat.to_homogeneous();

                    rotation = rotation * rotation_quat;
                }
            }

            // ********** combine **********
            let mut trans = Matrix4::<f32>::identity();
            trans = trans * translation;
            trans = trans * rotation;

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
                let rotation = Self::get_rotation_matrix_from_vector(rotation);
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

    pub fn apply_scale_all_axes(&mut self, scale: f32, multiply: bool)
    {
        self.apply_scale(Vector3::<f32>::new(scale, scale, scale), multiply);
    }

    pub fn set_scale(&mut self, scale: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.scale = scale;

        // if its zero -> inverse matrix can not be calculated
        if math::approx_zero(data.scale.x) { data.scale.x = 0.00000001; }
        if math::approx_zero(data.scale.y) { data.scale.y = 0.00000001; }
        if math::approx_zero(data.scale.z) { data.scale.z = 0.00000001; }

        if !data.transform_vectors
        {
            let scale = Matrix4::new_nonuniform_scaling(&scale);
            data.trans = scale;

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
            let rotation = Self::get_rotation_matrix_from_vector(rotation);
            data.trans = data.trans * rotation;
        }

        self.calc_transform();
    }

    pub fn set_rotation(&mut self, rotation: Vector3<f32>)
    {
        let data = self.data.get_mut();

        data.rotation = rotation;

        if !data.transform_vectors
        {
            let rotation = Self::get_rotation_matrix_from_vector(rotation);
            data.trans = data.trans * rotation;
        }

        self.calc_transform();
    }

    pub fn get_rotation_matrix_from_vector(rotation: Vector3<f32>) -> Matrix4<f32>
    {
        let rotation_x  = Rotation3::from_euler_angles(rotation.x, 0.0, 0.0).to_homogeneous();
        let rotation_y  = Rotation3::from_euler_angles(0.0, rotation.y, 0.0).to_homogeneous();
        let rotation_z  = Rotation3::from_euler_angles(0.0, 0.0, rotation.z).to_homogeneous();

        // yaw, pitch, roll
        let mut rotation = rotation_z;
        rotation = rotation * rotation_y;
        rotation = rotation * rotation_x;

        rotation
    }

    pub fn apply_rotation_quaternion(&mut self, rotation: Vector4<f32>, local: bool)
    {
        let data = self.data.get_mut();

        if data.rotation_quat.is_none()
        {
            data.rotation_quat = Some(rotation)
        }
        else
        {
            let data_rot_quat = data.rotation_quat.as_mut().unwrap();

            let new_rotation = UnitQuaternion::new_normalize(Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z));
            let current_rotation = UnitQuaternion::new_normalize(Quaternion::new(data_rot_quat.w, data_rot_quat.x, data_rot_quat.y, data_rot_quat.z));

            // local vs global rotation: just the multiplication orde rmatters
            // https://discussions.unity.com/t/understanding-rotations-in-local-and-world-space-quaternions/487221/2

            let new_rot_quat = if local
            {
                // local rotation
                (current_rotation * new_rotation).quaternion().coords
            }
            else
            {
                // global rotation
                (new_rotation * current_rotation).quaternion().coords
            };

            data_rot_quat.x = new_rot_quat.x;
            data_rot_quat.y = new_rot_quat.y;
            data_rot_quat.z = new_rot_quat.z;
            data_rot_quat.w = new_rot_quat.w;
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

                if local
                {
                    // local rotation
                    data.trans = data.trans * rotation;
                }
                else
                {
                    // global rotation
                    data.trans = rotation * data.trans;
                };
            }
        }

        self.calc_transform();
    }

    pub fn set_rotation_quaternion(&mut self, rotation: Vector4<f32>)
    {
        let data = self.data.get_mut();

        if data.rotation_quat.is_none()
        {
            data.rotation_quat = Some(rotation)
        }
        else
        {
            let data_rot_quat = data.rotation_quat.as_mut().unwrap();

            let new_rotation = UnitQuaternion::new_normalize(Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z));

            // local vs global rotation: just the multiplication orde rmatters
            // https://discussions.unity.com/t/understanding-rotations-in-local-and-world-space-quaternions/487221/2

            let new_rot_quat = new_rotation.quaternion().coords;

            data_rot_quat.x = new_rot_quat.x;
            data_rot_quat.y = new_rot_quat.y;
            data_rot_quat.z = new_rot_quat.z;
            data_rot_quat.w = new_rot_quat.w;
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

    pub fn convert_quaternion_to_euler_angles(&mut self)
    {
        let data = self.get_data_mut().get_mut();

        if let Some(rotation_quat) = &data.rotation_quat
        {
            let quaternion = UnitQuaternion::new_normalize
            (
                Quaternion::new
                (
                    rotation_quat.w,
                    rotation_quat.x,
                    rotation_quat.y,
                    rotation_quat.z,
                )
            );

            let (x, y, z) = quaternion.euler_angles();
            data.rotation = Vector3::new(x, y, z);
            data.rotation_quat = None;
        }
    }

    pub fn convert_euler_angles_to_quaternion(&mut self)
    {
        let data = self.get_data_mut().get_mut();

        let rotation_quat = UnitQuaternion::from_euler_angles(data.rotation.x, data.rotation.y, data.rotation.z);

        data.rotation_quat = Some(Vector4::<f32>::new(rotation_quat.coords.x, rotation_quat.coords.y, rotation_quat.coords.z, rotation_quat.coords.w));
        data.rotation = Vector3::zeros();
    }
}

#[typetag::serde]
impl Component for Transformation
{
    component_impl_default!();
    component_impl_no_update!();
    component_impl_no_cleanup_node!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        self.calc_transform();
    }

    fn instantiable() -> bool
    {
        true
    }

    fn duplicatable(&self) -> bool
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

    fn duplicate(&self) -> Option<crate::state::scene::components::component::ComponentItem>
    {
        let source = self.as_any().downcast_ref::<Transformation>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let mut transformation = Transformation
        {
            base: ComponentBase::duplicate(source.get_base()),

            data: ChangeTracker::new(source.get_data().clone()),

            ui_lock_translation: false,
            ui_lock_rotation: false,
            ui_lock_rotation_quat: false,
            ui_lock_scale: true,
        };

        transformation.data.force_change();

        Some(Arc::new(RwLock::new(Box::new(transformation))))
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        let mut changed = false;

        let mut pos;
        let mut rot;
        let mut rot_quat;
        let mut scale;
        let mut inheritance;
        let mut transform_vectors;

        {
            let data = self.get_data();

            pos = data.position;
            rot = data.rotation;
            rot_quat = data.rotation_quat;
            scale = data.scale;
            inheritance = data.parent_inheritance;
            transform_vectors = data.transform_vectors;

            ui.vertical(|ui|
            {
                changed = ui.checkbox(&mut inheritance, "parent transformation inheritance").changed() || changed;
                changed = ui.checkbox(&mut transform_vectors, "use vectors").changed() || changed;

                ui.horizontal(|ui|
                {
                    ui.label("Position: ");
                    let changed_x = ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_translation, "🔒").on_hover_text("same position value for all coordinates");

                    if self.ui_lock_translation  && changed_x { pos.y = pos.x; pos.z = pos.x; }
                    if self.ui_lock_translation  && changed_y { pos.x = pos.y; pos.z = pos.y; }
                    if self.ui_lock_translation  && changed_z { pos.x = pos.z; pos.y = pos.z; }

                    changed = changed_x || changed_y || changed_z || changed;
                });
                ui.horizontal(|ui|
                {
                    ui.label("Rotation\n(Euler): ");
                    let changed_x = ui.add(egui::DragValue::new(&mut rot.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut rot.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut rot.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_rotation, "🔒").on_hover_text("same rotation value for all coordinates");

                    if self.ui_lock_rotation  && changed_x { rot.y = rot.x; rot.z = rot.x; }
                    if self.ui_lock_rotation  && changed_y { rot.x = rot.y; rot.z = rot.y; }
                    if self.ui_lock_rotation  && changed_z { rot.x = rot.z; rot.y = rot.z; }

                    changed = changed_x || changed_y || changed_z || changed;
                });

                if let Some(rot_quat) = rot_quat.as_mut()
                {
                    ui.horizontal(|ui|
                    {
                        ui.label("Rotation\n(Quaternion): ");
                        let changed_x = ui.add(egui::DragValue::new(&mut rot_quat.x).speed(0.1).prefix("x: ")).changed();
                        let changed_y = ui.add(egui::DragValue::new(&mut rot_quat.y).speed(0.1).prefix("y: ")).changed();
                        let changed_z = ui.add(egui::DragValue::new(&mut rot_quat.z).speed(0.1).prefix("z: ")).changed();
                        let changed_w = ui.add(egui::DragValue::new(&mut rot_quat.w).speed(0.1).prefix("w: ")).changed();
                        ui.toggle_value(&mut self.ui_lock_rotation_quat, "🔒").on_hover_text("same rotation value for all coordinates (x, y, z)");

                        if self.ui_lock_rotation_quat  && changed_x { rot_quat.y = rot_quat.x; rot_quat.z = rot_quat.x; }
                        if self.ui_lock_rotation_quat  && changed_y { rot_quat.x = rot_quat.y; rot_quat.z = rot_quat.y; }
                        if self.ui_lock_rotation_quat  && changed_z { rot_quat.x = rot_quat.z; rot_quat.y = rot_quat.z; }

                        changed = changed_x || changed_y || changed_z || changed_w || changed;

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
                    let changed_x = ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: ")).changed();
                    let changed_y = ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: ")).changed();
                    let changed_z = ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: ")).changed();
                    ui.toggle_value(&mut self.ui_lock_scale, "🔒").on_hover_text("same scaling value for all coordinates");

                    if self.ui_lock_scale  && changed_x { scale.y = scale.x; scale.z = scale.x; }
                    if self.ui_lock_scale  && changed_y { scale.x = scale.y; scale.z = scale.y; }
                    if self.ui_lock_scale  && changed_z { scale.x = scale.z; scale.y = scale.z; }

                    changed = changed_x || changed_y || changed_z || changed;

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

                ui.separator();

                if ui.button("convert euler to quaternion").clicked()
                {
                    self.convert_euler_angles_to_quaternion();
                }

                if ui.button("convert quaternion to euler").clicked()
                {
                    self.convert_quaternion_to_euler_angles();
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
            data.get_mut().transform_vectors = transform_vectors;
            self.calc_transform();
        }

        let data = self.get_data();

        if data.animation_position.is_some() || data.animation_rotation_quat.is_some() || data.animation_scale.is_some()
        {
            ui.separator();
            ui.label(RichText::new("Animation Transformation:").strong());

            ui.add_enabled_ui(false, |ui|
            {
                if let Some(animation_position) = data.animation_position.clone()
                {
                    let mut pos = animation_position;
                    ui.horizontal(|ui|
                    {
                        ui.label("Position: ");
                        ui.add(egui::DragValue::new(&mut pos.x).speed(0.1).prefix("x: "));
                        ui.add(egui::DragValue::new(&mut pos.y).speed(0.1).prefix("y: "));
                        ui.add(egui::DragValue::new(&mut pos.z).speed(0.1).prefix("z: "));
                    });
                }

                if let Some(animation_rotation_quat) = data.animation_rotation_quat.clone()
                {
                    let mut rot_quat = animation_rotation_quat;
                    ui.horizontal(|ui|
                    {
                        ui.label("Rotation\n(Quaternion): ");
                        ui.add(egui::DragValue::new(&mut rot_quat.x).speed(0.1).prefix("x: "));
                        ui.add(egui::DragValue::new(&mut rot_quat.y).speed(0.1).prefix("y: "));
                        ui.add(egui::DragValue::new(&mut rot_quat.z).speed(0.1).prefix("z: "));
                        ui.add(egui::DragValue::new(&mut rot_quat.w).speed(0.1).prefix("w: "));
                    });
                }

                if let Some(animation_scale) = data.animation_scale.clone()
                {
                    let mut scale = animation_scale;
                    ui.horizontal(|ui|
                    {
                        ui.label("Scale: ");
                        ui.add(egui::DragValue::new(&mut scale.x).speed(0.1).prefix("x: "));
                        ui.add(egui::DragValue::new(&mut scale.y).speed(0.1).prefix("y: "));
                        ui.add(egui::DragValue::new(&mut scale.z).speed(0.1).prefix("z: "));
                    });
                }
            });
        }
    }
}