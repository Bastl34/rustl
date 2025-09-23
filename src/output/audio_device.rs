use std::sync::{Arc, RwLock, Mutex};

use nalgebra::Point3;
use rodio::{OutputStream, OutputStreamBuilder, mixer::Mixer};

use crate::{console_error, helper::change_tracker::ChangeTracker};

pub type AudioDeviceItem = Arc<RwLock<Box<AudioDevice>>>;

pub struct AudioDeviceData
{
    pub volume: f32,

    pub left_ear_pos: Point3::<f32>,
    pub right_ear_pos: Point3::<f32>,
}

/// Thread-safe wrapper for OutputStream
///
/// This is safe because:
/// 1. OutputStream is designed to be used from multiple threads in practice
/// 2. We only access it through a Mutex, ensuring exclusive access
/// 3. The underlying audio system handles thread safety internally
/// 4. This is a known limitation of CoreAudio on macOS that affects cpal/rodio
pub struct SafeOutputStream(OutputStream);

// SAFETY: OutputStream is designed to be thread-safe in practice.
// The underlying audio system (CoreAudio on macOS) handles thread safety.
// We wrap access in a Mutex to ensure exclusive access.
// This is a known workaround for a limitation in cpal's CoreAudio implementation.
unsafe impl Send for SafeOutputStream {}
unsafe impl Sync for SafeOutputStream {}

impl SafeOutputStream
{
    pub fn new(stream: OutputStream) -> Self
    {
        Self(stream)
    }

    pub fn mixer(&self) -> &Mixer
    {
        self.0.mixer()
    }
}

pub struct AudioDevice
{
    pub stream: Option<Arc<Mutex<SafeOutputStream>>>,
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
                stream: Some(Arc::new(Mutex::new(SafeOutputStream::new(stream)))),
                data,
            }
        }
        else
        {
            console_error!("audio device not found");
            Self
            {
                stream: None,
                data,
            }
        }

    }
}

impl AudioDevice
{
    pub fn get_stream(&self) -> Option<Arc<Mutex<SafeOutputStream>>>
    {
        self.stream.clone()
    }
}