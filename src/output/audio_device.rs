use std::sync::{Arc, RwLock};

use nalgebra::Point3;
use rodio::{OutputStream, OutputStreamBuilder};

use crate::helper::change_tracker::ChangeTracker;

pub type AudioDeviceItem = Arc<RwLock<Box<AudioDevice>>>;

pub struct AudioDeviceData
{
    pub volume: f32,

    pub left_ear_pos: Point3::<f32>,
    pub right_ear_pos: Point3::<f32>,
}

pub struct AudioDevice
{
    pub stream: Option<OutputStream>,
    pub data: ChangeTracker<AudioDeviceData>
}

impl Default for AudioDevice
{
    fn default() -> Self
    {
        let data = ChangeTracker::new(AudioDeviceData
        {
            volume: 1.0,
            left_ear_pos: Point3::<f32>::new(-1.0, 0.0, 0.0),
            right_ear_pos: Point3::<f32>::new(1.0, 0.0, 0.0),
        });

        if let Ok(stream) = OutputStreamBuilder::open_default_stream()
        {
            Self
            {
                stream: Some(stream),
                data,
            }
        }
        else
        {
            dbg!("audio device not found");
            Self
            {
                stream: None,
                data,
            }
        }

    }
}