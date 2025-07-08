use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::state::{scene::node::NodeItem, state::InputOutput};

//pub type CameraControllerBox = Box<dyn SerializableCameraController + Send + Sync>;
pub type CameraControllerBox = Box<dyn CameraController>;

#[typetag::serde(tag = "type")]
pub trait CameraController: Any + Send + Sync
{
    fn get_base(&self) -> &CameraControllerBase;
    fn get_base_mut(&mut self) -> &mut CameraControllerBase;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn serializable(&self) -> bool { true }
    fn deserializable(&self) -> bool { true }

    fn ui(&mut self, ui: &mut egui::Ui);

    fn update(&mut self, node: Option<NodeItem>, scene: &mut crate::state::scene::scene::Scene, io: &mut InputOutput, cam_data: &mut crate::helper::change_tracker::ChangeTracker<crate::state::scene::camera::CameraData>, frame_scale: f32) -> bool;
}

#[derive(Serialize, Deserialize)]
pub struct CameraControllerBase
{
    pub is_enabled: bool,
    pub name: String,
    pub icon: String,
}

impl CameraControllerBase
{
    pub fn new(name: String, icon: String) -> CameraControllerBase
    {
        CameraControllerBase
        {
            name,
            icon,
            is_enabled: true
        }
    }
}

// ******************** default implementations ********************

#[macro_export]
macro_rules! camera_controller_impl_default
{
    () =>
    {
        fn as_any(&self) -> &dyn std::any::Any
        {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any
        {
            self
        }

        fn get_base(&self) -> &CameraControllerBase
        {
            &self.base
        }

        fn get_base_mut(&mut self) -> &mut CameraControllerBase
        {
            &mut self.base
        }
    };
}

#[macro_export]
macro_rules! impl_unserializable
{
    ($t:ty) =>
    {
        impl serde::Serialize for $t
        {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom(concat!(stringify!($t), " cannot be serialized")))
            }
        }

        impl<'de> serde::Deserialize<'de> for $t
        {
            fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Err(serde::de::Error::custom(concat!(stringify!($t), " cannot be deserialized")))
            }
        }
    };
}

// usage:
// impl_unserializable!(MyCameraController);
