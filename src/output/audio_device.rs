use std::sync::{Arc, RwLock, Mutex};

use nalgebra::Point3;
use rodio::{MixerDeviceSink, DeviceSinkBuilder, mixer::Mixer};

use crate::{console_error, helper::change_tracker::ChangeTracker, state::scene::components::sound::SoundCategory};

pub type AudioDeviceItem = Arc<RwLock<Box<AudioDevice>>>;

pub struct AudioDeviceData
{
    pub mute: bool,
    pub volume: f32,
    pub effects_volume: f32,
    pub music_volume: f32,

    // left and right ear of every active camera (split screen) - a spatial sound is heard by the nearest pair
    pub listeners: Vec<(Point3<f32>, Point3<f32>)>,

    // the side of the screen each listener's camera is on: -1 left half, 1 right half, 0 neither (same order as listeners)
    pub listener_sides: Vec<f32>,

    // split screen: a sound heard by the camera of the left half plays only on the left channel (the right one only on the right)
    pub split_pan: bool,
}

impl AudioDeviceData
{
    // master volume times the volume of the category
    pub fn category_volume(&self, category: SoundCategory) -> f32
    {
        if self.mute
        {
            return 0.0;
        }

        self.volume * match category
        {
            SoundCategory::Effects => self.effects_volume,
            SoundCategory::Music => self.music_volume,
        }
    }

    pub fn nearest_listener(&self, position: &Point3<f32>) -> Option<(Point3<f32>, Point3<f32>)>
    {
        self.nearest_listener_side(position).map(|(ears, _)| ears)
    }

    // the nearest ears and their side of the screen (0 without split_pan or a side)
    pub fn nearest_listener_side(&self, position: &Point3<f32>) -> Option<((Point3<f32>, Point3<f32>), f32)>
    {
        let index = (0..self.listeners.len()).min_by(|a, b|
        {
            let (a_left, a_right) = &self.listeners[*a];
            let (b_left, b_right) = &self.listeners[*b];
            let a = nalgebra::distance(&nalgebra::center(a_left, a_right), position);
            let b = nalgebra::distance(&nalgebra::center(b_left, b_right), position);
            a.total_cmp(&b)
        })?;

        let side = if self.split_pan && self.listeners.len() > 1 { self.listener_sides.get(index).copied().unwrap_or(0.0) } else { 0.0 };
        Some((self.listeners[index], side))
    }
}

/// Thread-safe wrapper for OutputStream
///
/// This is safe because:
/// 1. OutputStream is designed to be used from multiple threads in practice
/// 2. We only access it through a Mutex, ensuring exclusive access
/// 3. The underlying audio system handles thread safety internally
/// 4. This is a known limitation of CoreAudio on macOS that affects cpal/rodio
pub struct SafeOutputStream(MixerDeviceSink);

// SAFETY: OutputStream is designed to be thread-safe in practice.
// The underlying audio system (CoreAudio on macOS) handles thread safety.
// We wrap access in a Mutex to ensure exclusive access.
// This is a known workaround for a limitation in cpal's CoreAudio implementation.
unsafe impl Send for SafeOutputStream {}
unsafe impl Sync for SafeOutputStream {}

impl SafeOutputStream
{
    pub fn new(stream: MixerDeviceSink) -> Self
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
            mute: false,
            volume: 1.0,
            effects_volume: 1.0,
            music_volume: 1.0,
            listeners: vec![(Point3::<f32>::new(-1.0, 0.0, 0.0), Point3::<f32>::new(1.0, 0.0, 0.0))],
            listener_sides: vec![0.0],
            split_pan: false,
        });

        if let Ok(mut stream) = DeviceSinkBuilder::open_default_sink()
        {
            // no note on stdout when the app closes
            stream.log_on_drop(false);
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