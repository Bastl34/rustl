#![allow(dead_code)]

use std::{sync::{Arc, RwLock}};

use egui::RichText;
use nalgebra::{distance, Point3};
use rodio::{Player, Source, SpatialPlayer};
use serde::{Deserialize, Serialize};
use web_time::Duration;

use crate::{component_impl_default, component_impl_no_cleanup_node, console_error, console_warning, helper::{change_tracker::ChangeTracker, math::approx_zero, option_or_id::OptionOrId}, output::audio_device::AudioDeviceItem, state::{resources::sound_source::{SoundSourceItem, find_sound_source}, scene::node::{InstanceItemArc, NodeItem}, state::InputOutput}};
use crate::state::resources::sound_source::Decodable;
use crate::state::scene::exporter::serialization_helper;

use super::component::{Component, ComponentBase, ComponentItem};

#[derive(PartialEq, Copy, Clone, Serialize, Deserialize)]
pub enum SoundType
{
    Spatial,
    Stereo
}

#[derive( Copy, Clone, Serialize, Deserialize)]
pub struct SoundData
{
    pub sound_type: SoundType,

    pub looped: bool,
    pub volume: f32,
    pub speed: f32,

    pub spatial_distance_scale: f32,

    pub delete_after_playback: bool
}
#[derive(Serialize, Deserialize)]
pub struct Sound
{
    base: ComponentBase,

    data: ChangeTracker<SoundData>,

    #[serde(serialize_with = "serialization_helper::serialize_sound_source", deserialize_with = "serialization_helper::deserialize_sound_source")]
    pub sound_source: OptionOrId<SoundSourceItem>,
    pub duration: f32,

    #[serde(skip, default)]
    audio_device: Option<AudioDeviceItem>,

    #[serde(skip, default)]
    player: Option<Player>,

    #[serde(skip, default)]
    player_spatial: Option<SpatialPlayer>,

    // set by a controller every frame on top of volume and speed - not saved
    #[serde(skip, default = "default_one")]
    runtime_gain: f32,
    #[serde(skip, default = "default_one")]
    runtime_pitch: f32,
    #[serde(skip, default)]
    runtime_changed: bool,
}

fn default_one() -> f32 { 1.0 }

impl Sound
{
    pub fn new(name: &str, sound_source: SoundSourceItem, sound_type: SoundType, looped: bool) -> Sound
    {
        let mut sound = Sound
        {
            base: ComponentBase::new(name.to_string(), "Sound".to_string(), "🔊".to_string()),

            sound_source: OptionOrId::Some(sound_source.clone()),
            duration: 0.0,

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

            player: None,
            player_spatial: None,

            runtime_gain: 1.0,
            runtime_pitch: 1.0,
            runtime_changed: false,
        };

        sound.set_sound_source(sound_source.clone());

        sound
    }

    pub fn new_empty(name: &str) -> Sound
    {
        let sound = Sound
        {
            base: ComponentBase::new(name.to_string(), "Sound".to_string(), "🔊".to_string()),

            sound_source: OptionOrId::None,
            duration: 0.0,

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

            player: None,
            player_spatial: None,

            runtime_gain: 1.0,
            runtime_pitch: 1.0,
            runtime_changed: false,
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

    // gain and pitch factor of a controller, e.g. the engine rpm - 1.0 plays the sound as set
    pub fn set_runtime_modulation(&mut self, gain: f32, pitch: f32)
    {
        if self.runtime_gain != gain || self.runtime_pitch != pitch
        {
            self.runtime_gain = gain;
            self.runtime_pitch = pitch;
            self.runtime_changed = true;
        }
    }

    pub fn reset(&mut self)
    {
        if let Some(player) = &mut self.player
        {
            player.stop();
        }

        if let Some(player) = &mut self.player_spatial
        {
            player.stop();
        }

        self.player = None;
        self.player_spatial = None;
    }

    pub fn set_sound_source(&mut self, sound_source: SoundSourceItem)
    {
        self.reset();

        self.sound_source = OptionOrId::Some(sound_source.clone());
        self.audio_device = Some(sound_source.read().unwrap().audio_device.clone());

        let sound_source = sound_source.read().unwrap();
        let audio_device = sound_source.audio_device.read().unwrap();

        let mut sink = None;
        let mut sink_spatial = None;

        let data = self.data.get_ref();
        if let Some(stream_arc) = audio_device.get_stream()
        {
            let stream = stream_arc.lock().unwrap();

            if data.sound_type == SoundType::Stereo
            {
                let s = rodio::Player::connect_new(stream.mixer());
                if let Some(decoder) = sound_source.decoder()
                {
                    if let Some(total_duration) = decoder.total_duration()
                    {
                        self.duration = total_duration.as_secs_f32();
                    }

                    if data.looped
                    {
                        s.append(decoder.repeat_infinite());
                    }
                    else
                    {
                        s.append(decoder);
                    }
                }
                else
                {
                    console_error!("Sound: Unable to create decoder for sound source {}", sound_source.name);
                }
                s.pause();
                sink = Some(s);
            }
            else
            {
                let s = rodio::SpatialPlayer::connect_new(stream.mixer(), [0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
                if let Some(decoder) = sound_source.decoder()
                {
                    if let Some(total_duration) = decoder.total_duration()
                    {
                        self.duration = total_duration.as_secs_f32();
                    }

                    if data.looped
                    {
                        s.append(decoder.repeat_infinite());
                    }
                    else
                    {
                        s.append(decoder);
                    }
                }
                else
                {
                    console_error!("Sound: Unable to create decoder for sound source {}", sound_source.name);
                }
                s.pause();
                sink_spatial = Some(s);
            }
        }

        self.player = sink;
        self.player_spatial = sink_spatial;

        self.update_state(None, None, true);
    }

    pub fn running(&self) -> bool
    {
        if let Some(player) = &self.player
        {
            return !player.is_paused() && !player.empty();
        }

        if let Some(player) = &self.player_spatial
        {
            return !player.is_paused() && !player.empty();
        }

        false
    }

    pub fn stopped(&self) -> bool
    {
        if let Some(player) = &self.player
        {
            return player.empty();
        }

        if let Some(player) = &self.player_spatial
        {
            return player.empty();
        }

        false
    }

    pub fn start(&mut self)
    {
        if self.stopped()
        {
            self.set_sound_source(self.sound_source.clone().unwrap());
        }

        if let Some(player) = &mut self.player
        {
            player.play();
        }

        if let Some(player) = &mut self.player_spatial
        {
            player.play();
        }
    }

    pub fn stop(&mut self)
    {
        if let Some(player) = &mut self.player
        {
            player.stop()
        }

        if let Some(player) = &mut self.player_spatial
        {
            player.stop();
        }
    }

    pub fn pause(&mut self)
    {
        if let Some(player) = &mut self.player
        {
            player.pause()
        }

        if let Some(player) = &mut self.player_spatial
        {
            player.pause();
        }
    }

    pub fn sound_time(&self) -> f32
    {
        if let Some(player) = &self.player
        {
            let pos = player.get_pos();

            if self.get_data().looped && !approx_zero(self.duration) && pos >= Duration::from_secs_f32(self.duration)
            {
                return pos.as_secs_f32() % self.duration;
            }

            return pos.as_secs_f32();
        }

        if let Some(player) = &self.player_spatial
        {
            let pos = player.get_pos();

            if self.get_data().looped && !approx_zero(self.duration) && pos >= Duration::from_secs_f32(self.duration)
            {
                return pos.as_secs_f32() % self.duration;
            }

            return pos.as_secs_f32();
        }

        0.0
    }

    pub fn set_current_time(&mut self, time: f32)
    {
        if let Some(player) = &mut self.player
        {
            let pos = Duration::from_secs_f32(time);
            let res = player.try_seek(pos);
            if res.is_err()
            {
                console_warning!("can not seek, because its not supported for this file");
                console_warning!("{:?}", res);
            }
        }

        if let Some(player) = &mut self.player_spatial
        {
            let pos = Duration::from_secs_f32(time);
            let res = player.try_seek(pos);
            if res.is_err()
            {
                console_warning!("can not seek, because its not supported for this file");
                console_warning!("{:?}", res);
            }
        }
    }

    pub fn update_state(&mut self, node: Option<NodeItem>, instance: Option<&InstanceItemArc>, force: bool)
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

        let runtime_change = std::mem::take(&mut self.runtime_changed);
        let (data, change) = self.data.consume_borrow();

        let is_spatial = self.player_spatial.is_some();

        if !audio_device_change && !change && !runtime_change && !force && !is_spatial
        {
            return;
        }

        let volume = audio_device.data.get_ref().volume * data.volume * self.runtime_gain;
        let speed = data.speed * self.runtime_pitch;

        // default player
        if let Some(player) = &self.player
        {
            player.set_volume(volume);
            player.set_speed(speed);
        }

        // spatial player
        if let Some(player) = &self.player_spatial
        {
            player.set_volume(volume);
            player.set_speed(speed);

            let mut position = None;
            if let Some(instance) = instance
            {
                let instance = instance.read().unwrap();
                let transform = instance.get_cached_world_transform();
                position = Some(Point3::<f32>::new(transform.m14, transform.m24, transform.m34));

            }
            else if let Some(node) = &node
            {
                let node = node.read().unwrap();
                let transform = node.get_full_transform();
                position = Some(Point3::<f32>::new(transform.m14, transform.m24, transform.m34));
            }

            // split screen: the camera nearest to the sound hears it
            if let Some((position, (left_pos, right_pos))) = position.and_then(|position| Some((position, audio_device_data.nearest_listener(&position)?)))
            {
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

                player.set_emitter_position(pos);
                player.set_left_ear_position(left);
                player.set_right_ear_position(right);
            }
        }
    }
}

impl Drop for Sound
{
    fn drop(&mut self)
    {
        self.stop();
    }
}

#[typetag::serde]
impl Component for Sound
{
    component_impl_default!();
    component_impl_no_cleanup_node!();

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        // not saved
        self.base.component_name = "Sound".to_string();
        self.base.icon = "🔊".to_string();

        // a missing resource keeps its uuid, so saving does not lose the assignment
        if let Some(uuid) = self.sound_source.id().map(str::to_string)
        {
            match find_sound_source(&context.sound_sources, &uuid)
            {
                Some(sound_source) => self.set_sound_source(sound_source),
                None => { console_error!("Sound '{}': no sound resource with the uuid {}", self.base.name, uuid); },
            }
        }
    }

    fn saved_with_node(&self) -> bool
    {
        true
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
        }
    }

    fn duplicate(&self) -> Option<ComponentItem>
    {
        let source = self.as_any().downcast_ref::<Sound>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let mut sound = Sound
        {
            base: ComponentBase::duplicate(source.get_base()),

            sound_source: source.sound_source.clone(),
            duration: source.duration,

            data: ChangeTracker::new(source.get_data().clone()),

            audio_device: None,

            player: None,
            player_spatial: None,

            runtime_gain: 1.0,
            runtime_pitch: 1.0,
            runtime_changed: false,
        };

        if let Some(sound_source) = source.sound_source.as_ref()
        {
            sound.set_sound_source(sound_source.clone());
        }

        Some(Arc::new(RwLock::new(Box::new(sound))))
    }

    fn update(&mut self, node: NodeItem, _io: &mut InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
    {
        self.update_state(Some(node), None, false);
    }

    fn update_instance(&mut self, node: Option<NodeItem>, instance: &InstanceItemArc, _io: &mut InputOutput, _time: u128, _frame_scale: f32, _frame: u64)
    {
        self.update_state(node, Some(instance), false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _node: Option<NodeItem>)
    {
        match &self.sound_source
        {
            OptionOrId::Some(sound_source) => sound_source.read().unwrap().ui_info(ui),
            OptionOrId::Id(uuid) =>
            {
                ui.label(RichText::new(format!("⚠ no sound resource with the uuid {}", uuid)).color(egui::Color32::LIGHT_RED));
                return;
            },
            OptionOrId::None =>
            {
                ui.label(RichText::new("no sound resource - pick one in the sound settings").color(egui::Color32::GRAY));
                return;
            },
        }

        if !approx_zero(self.duration)
        {
            ui.label(format!("Duration: {}", self.duration));
        }
        else
        {
            ui.label(format!("Duration: unkown"));
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

        ui.horizontal(|ui|
        {
            if !approx_zero(self.duration)
            {
                ui.label("Progress: ");
                let mut time = self.sound_time() * speed;
                if ui.add(egui::Slider::new(&mut time, 0.0..=self.duration).fixed_decimals(2).clamping(egui::SliderClamping::Edits).text("s")).changed()
                {
                    self.set_current_time(time);
                }
            }
        });

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
                let running = self.running();
                self.set_sound_source(self.sound_source.clone().unwrap());

                if running
                {
                    self.start();
                }
            }
        }
    }
}