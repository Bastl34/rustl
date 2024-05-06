use std::sync::{Arc, RwLock};

use nalgebra::Point3;
use rodio::{OutputStream, OutputStreamHandle};

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
    pub stream_handle: Option<OutputStreamHandle>,
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

        if let Ok((stream, stream_handle)) = OutputStream::try_default()
        {
            // leaking stream here becase it is not able to Send it
            // also it should be fine leaking its just created once
            // otherwise the audio will stop
            // https://github.com/bevyengine/bevy/blob/main/crates/bevy_audio/src/audio_output.rs#L15
            // https://github.com/RustAudio/cpal/issues/818
            // https://github.com/RustAudio/cpal/issues/793
            std::mem::forget(stream);

            Self
            {
                stream_handle: Some(stream_handle),
                data

            }
        }
        else
        {
            dbg!("audio device not found");
            Self
            {
                stream_handle: None,
                data
            }
        }
    }
}