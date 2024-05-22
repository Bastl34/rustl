use egui::RichText;
use nalgebra::{distance, Point3};
use rodio::{Sink, Source, SpatialSink};

use crate::{component_impl_default, helper::change_tracker::ChangeTracker, input::input_manager::InputManager, output::audio_device::AudioDeviceItem, state::scene::{node::{InstanceItemArc, NodeItem}, sound_source::SoundSourceItem}};
use crate::state::scene::sound_source::Decodable;

use super::component::{ComponentBase, Component};

#[derive(PartialEq, Copy, Clone)]
pub enum SoundType
{
    Spatial,
    Stereo
}

pub struct SoundData
{
    pub sound_type: SoundType,

    pub looped: bool,
    pub volume: f32,
    pub speed: f32,

    pub spatial_distance_scale: f32,

    pub delete_after_playback: bool
}

pub struct Sound
{
    base: ComponentBase,

    data: ChangeTracker<SoundData>,

    pub sound_source: Option<SoundSourceItem>,

    audio_device: Option<AudioDeviceItem>,

    sink: Option<Sink>,
    sink_spatial: Option<SpatialSink>,
}

impl Sound
{
    pub fn new(id: u64, name: &str, sound_source: SoundSourceItem, sound_type: SoundType, looped: bool) -> Sound
    {
        let mut sound = Sound
        {
            base: ComponentBase::new(id, name.to_string(), "Sound".to_string(), "🔊".to_string()),

            sound_source: Some(sound_source.clone()),

            data: ChangeTracker::new(SoundData
            {
                sound_type,
                looped,
                volume: 1.0,
                speed: 1.0,

                spatial_distance_scale: 1.0,

                delete_after_playback: false,
            }),

            audio_device: None,

            sink: None,
            sink_spatial: None,
        };

        sound.set_sound_source(sound_source.clone());

        sound
    }

    pub fn new_empty(id: u64, name: &str) -> Sound
    {
        let sound = Sound
        {
            base: ComponentBase::new(id, name.to_string(), "Sound".to_string(), "🔊".to_string()),

            sound_source: None,

            data: ChangeTracker::new(SoundData
            {
                sound_type: SoundType::Stereo,
                looped: false,
                volume: 1.0,
                speed: 1.0,

                spatial_distance_scale: 1.0,

                delete_after_playback: false
            }),

            audio_device: None,

            sink: None,
            sink_spatial: None,
        };

        sound
    }

    pub fn get_data(&self) -> &SoundData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<SoundData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<SoundData>
    {
        &mut self.data
    }

    pub fn reset(&mut self)
    {
        if let Some(sink) = &mut self.sink
        {
            sink.stop();
        }

        if let Some(sink) = &mut self.sink_spatial
        {
            sink.stop();
        }

        self.sink = None;
        self.sink_spatial = None;
    }

    pub fn set_sound_source(&mut self, sound_source: SoundSourceItem)
    {
        self.reset();

        self.sound_source = Some(sound_source.clone());
        self.audio_device = Some(sound_source.read().unwrap().audio_device.clone());

        let sound_source = sound_source.read().unwrap();
        let audio_device = sound_source.audio_device.read().unwrap();
        let stream_handle = audio_device.stream_handle.as_ref();

        let mut sink = None;
        let mut sink_spatial = None;

        if let Some(stream_handle) = stream_handle
        {
            let data = self.data.get_ref();

            if data.sound_type == SoundType::Stereo
            {
                let s = rodio::Sink::try_new(stream_handle).unwrap();

                if data.looped
                {
                    s.append(sound_source.decoder().repeat_infinite());
                }
                else
                {
                    s.append(sound_source.decoder());
                }

                s.pause();

                sink = Some(s);
            }
            else
            {
                let s = rodio::SpatialSink::try_new(stream_handle, [0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();

                if data.looped
                {
                    s.append(sound_source.decoder().repeat_infinite());
                }
                else
                {
                    s.append(sound_source.decoder());
                }

                s.pause();

                sink_spatial = Some(s);
            }
        }

        self.sink = sink;
        self.sink_spatial = sink_spatial;

        self._update(None, None, true);
    }

    pub fn running(&self) -> bool
    {
        if let Some(sink) = &self.sink
        {
            return !sink.is_paused() && !sink.empty();
        }

        if let Some(sink) = &self.sink_spatial
        {
            return !sink.is_paused() && !sink.empty();
        }

        false
    }

    pub fn stopped(&self) -> bool
    {
        if let Some(sink) = &self.sink
        {
            return sink.empty();
        }

        if let Some(sink) = &self.sink_spatial
        {
            return sink.empty();
        }

        false
    }

    pub fn start(&mut self)
    {
        if let Some(sink) = &mut self.sink
        {
            sink.play();
        }

        if let Some(sink) = &mut self.sink_spatial
        {
            sink.play();
        }
    }

    pub fn stop(&mut self)
    {
        if let Some(sink) = &mut self.sink
        {
            sink.stop()
        }

        if let Some(sink) = &mut self.sink_spatial
        {
            sink.stop();
        }
    }

    pub fn pause(&mut self)
    {
        if let Some(sink) = &mut self.sink
        {
            sink.pause()
        }

        if let Some(sink) = &mut self.sink_spatial
        {
            sink.pause();
        }
    }

    fn _update(&mut self, node: Option<NodeItem>, instance: Option<&InstanceItemArc>, force: bool)
    {
        if self.get_data().delete_after_playback && self.stopped()
        {
            self.get_base_mut().delete_later();
        }

        if self.audio_device.is_none()
        {
            return;
        }

        let audio_device = self.audio_device.as_ref().unwrap();
        let audio_device = audio_device.read().unwrap();

        let audio_device_change = audio_device.data.changed();
        let audio_device_data = audio_device.data.get_ref();

        let (data, change) = self.data.consume_borrow();

        let is_spatial = self.sink_spatial.is_some();

        if !audio_device_change && !change && !force && !is_spatial
        {
            return;
        }

        let volume = audio_device.data.get_ref().volume * data.volume;

        // default sink
        if let Some(sink) = &self.sink
        {
            sink.set_volume(volume);
            sink.set_speed(data.speed);
        }

        // spatial sink
        if let Some(sink) = &self.sink_spatial
        {
            sink.set_volume(audio_device.data.get_ref().volume * data.volume);
            sink.set_speed(data.speed);

            let mut position = None;
            if let Some(instance) = instance
            {
                let instance = instance.read().unwrap();
                let transform = instance.get_world_transform();
                position = Some(Point3::<f32>::new(transform.m14, transform.m24, transform.m34));

            }
            else if let Some(node) = &node
            {
                let node = node.read().unwrap();
                let transform = node.get_full_transform();
                position = Some(Point3::<f32>::new(transform.m14, transform.m24, transform.m34));
            }

            if let Some(position) = position
            {
                let left_pos = audio_device_data.left_ear_pos;
                let right_pos = audio_device_data.right_ear_pos;

                let dist_left = distance(&left_pos, &position);
                let dist_right = distance(&right_pos, &position);

                let emitter_pos;
                if dist_left < dist_right
                {
                    let mut emitter_vec = position - left_pos;
                    emitter_vec *= 1.0 / data.spatial_distance_scale;
                    emitter_pos = left_pos + emitter_vec;
                }
                else
                {
                    let mut emitter_vec = position - right_pos;
                    emitter_vec *= 1.0 / data.spatial_distance_scale;
                    emitter_pos = right_pos + emitter_vec;
                }

                let pos = [emitter_pos.x, emitter_pos.y, emitter_pos.z];
                let left = [left_pos.x, left_pos.y, left_pos.z];
                let right = [right_pos.x, right_pos.y, right_pos.z];

                sink.set_emitter_position(pos);
                sink.set_left_ear_position(left);
                sink.set_right_ear_position(right);
            }
        }
    }
}

impl Drop for Sound
{
    fn drop(&mut self)
    {
        dbg!("droppinggggggggggggggggggggggg sound");
        self.stop();
    }
}

impl Component for Sound
{
    component_impl_default!();

    fn instantiable() -> bool
    {
        true
    }

    fn set_enabled(&mut self, state: bool)
    {
        if self.base.is_enabled != state
        {
            self.base.is_enabled = state;
        }
    }

    fn update(&mut self, node: NodeItem, _input_manager: &mut InputManager, _time: u128, _frame_scale: f32, _frame: u64)
    {
        self._update(Some(node), None, false);
    }

    fn update_instance(&mut self, node: NodeItem, instance: &InstanceItemArc, _input_manager: &mut InputManager, _time: u128, _frame_scale: f32, _frame: u64)
    {
        self._update(Some(node), Some(instance), false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        if self.sound_source.is_none()
        {
            return;
        }

        if let Some(sound_source) = &self.sound_source
        {
            sound_source.read().unwrap().ui_info(ui);
        }

        {
            let is_pause = !self.running();
            let mut is_stopped = is_pause;
            let mut is_running = !is_pause;

            let icon_size = 20.0;
            ui.horizontal(|ui|
            {
                if ui.toggle_value(&mut is_stopped, RichText::new("⏹").size(icon_size)).on_hover_text("stop animation").clicked()
                {
                    self.stop();
                };

                if ui.toggle_value(&mut is_running, RichText::new("⏵").size(icon_size)).on_hover_text("play animation").clicked()
                {
                    if self.stopped()
                    {
                        self.set_sound_source(self.sound_source.clone().unwrap());
                    }
                    self.start();
                }

                if ui.toggle_value(&mut false, RichText::new("⏸").size(icon_size)).on_hover_text("pause animation").clicked()
                {
                    self.pause();
                }
            });
        }

        let mut changed = false;

        let mut volume;
        let mut speed;
        let mut looped;
        let mut sound_type;
        let mut spatial_distance_scale;
        let mut delete_after_playback;

        {
            let data = self.data.get_ref();

            volume = data.volume;
            speed = data.speed;
            looped = data.looped;
            sound_type = data.sound_type;
            spatial_distance_scale = data.spatial_distance_scale;
            delete_after_playback = data.delete_after_playback;
        }

        changed = ui.checkbox(&mut looped, "Loop").changed() || changed;
        changed = ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).text("Volume")).changed() || changed;
        changed = ui.add(egui::Slider::new(&mut speed, 0.01..=10.0).text("Speed")).changed() || changed;

        changed = ui.add(egui::Slider::new(&mut spatial_distance_scale, 0.01..=10.0).text("Spatial distance scale")).changed() || changed;

        ui.horizontal(|ui|
        {
            ui.label("Type:");
            changed = ui.radio_value(&mut sound_type, SoundType::Stereo, "Stereo").changed() || changed;
            changed = ui.radio_value(&mut sound_type, SoundType::Spatial, "Spatial").changed() || changed;
        });

        changed = ui.checkbox(&mut delete_after_playback, "Delete after playback").changed() || changed;

        if changed
        {
            let data = self.data.get_mut();

            let major_change = data.looped != looped;
            let major_change = major_change || data.sound_type != sound_type;

            data.volume = volume;
            data.looped = looped;
            data.speed = speed;
            data.sound_type = sound_type;
            data.spatial_distance_scale = spatial_distance_scale;
            data.delete_after_playback = delete_after_playback;

            if major_change
            {
                self.set_sound_source(self.sound_source.clone().unwrap());
            }
        }
    }
}