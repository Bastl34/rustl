use std::{f32::consts::PI, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{component_downcast_mut, console_warning, helper::option_or_id::OptionOrId, state::scene::{components::{component::{Component, ComponentItem}, sound::Sound}, exporter::serialization_helper::{deserialize_component, serialize_component}}};

// pitch range a loop may be played at before it sounds broken
const MIN_PITCH: f32 = 0.35;
const MAX_PITCH: f32 = 2.6;

// how often the random part of the wobble picks a new target, s
const WOBBLE_MIN_INTERVAL: f32 = 0.12;
const WOBBLE_MAX_INTERVAL: f32 = 0.45;
const WOBBLE_SMOOTHING: f32 = 6.0;
const WOBBLE_LFO_SPEED: f32 = 1.7; // rad/s

// tire sounds fade in fast and out a bit slower, 1/s - switching them per frame clicks
const TIRE_ATTACK: f32 = 18.0;
const TIRE_RELEASE: f32 = 7.0;

// the road noise reaches its full volume at this speed, m/s
const ROAD_FULL_SPEED: f32 = 30.0;

// share of the rpm step between two layers in which they are crossfaded - two tonal loops at the same pitch comb filter each other
const CROSSFADE_WIDTH: f32 = 0.4;

fn default_one() -> f32 { 1.0 }

#[derive(Serialize, Deserialize, Clone)]
pub struct EngineSoundLayer
{
    #[serde(default, serialize_with = "serialize_component", deserialize_with = "deserialize_component")]
    pub sound: OptionOrId<ComponentItem>, // a looped sound component of the vehicle node
    pub rpm: f32, // the rpm the loop was recorded at - it is pitched from there

    // 1 = recorded on throttle, 0 = off throttle (overrun) - the two groups are blended by the engine load
    #[serde(default = "default_one")]
    pub load: f32,
}

// Which sound components of the vehicle node play what - saved as their uuids. Volume, spatial and distance are set on the components.
#[derive(Serialize, Deserialize, Clone)]
pub struct VehicleSoundSettings
{
    pub enabled: bool,
    pub engine_layers: Vec<EngineSoundLayer>,

    #[serde(default, serialize_with = "serialize_component", deserialize_with = "deserialize_component")]
    pub squeal: OptionOrId<ComponentItem>, // tires sliding sideways or locked by the brakes
    #[serde(default, serialize_with = "serialize_component", deserialize_with = "deserialize_component")]
    pub road: OptionOrId<ComponentItem>, // rolling noise, rises with the speed

    // random rpm wobble, as a share of the rpm - keeps full throttle from turning into one flat tone
    pub pitch_variation: f32,
}

impl Default for VehicleSoundSettings
{
    fn default() -> Self
    {
        Self
        {
            enabled: true,
            engine_layers: vec![],
            squeal: OptionOrId::None,
            road: OptionOrId::None,
            pitch_variation: 0.02,
        }
    }
}

impl VehicleSoundSettings
{
    fn sounds_mut(&mut self) -> impl Iterator<Item = &mut OptionOrId<ComponentItem>>
    {
        self.engine_layers.iter_mut().map(|layer| &mut layer.sound).chain([&mut self.squeal, &mut self.road])
    }

    fn sounds(&self) -> impl Iterator<Item = &OptionOrId<ComponentItem>>
    {
        self.engine_layers.iter().map(|layer| &layer.sound).chain([&self.squeal, &self.road])
    }

    pub fn has_sounds(&self) -> bool
    {
        self.sounds().any(|sound| sound.is_some())
    }

    // after deserializing: uuids to the components of the vehicle node
    pub fn resolve(&mut self, components: &[ComponentItem])
    {
        for sound in self.sounds_mut()
        {
            resolve_sound_component(sound, components);
        }
    }

    // references to components that are no longer on the vehicle node are dropped - returns the components to silence
    pub fn release_detached(&mut self, attached: &[ComponentItem]) -> Vec<ComponentItem>
    {
        self.sounds_mut().filter_map(|sound| release_if_detached(sound, attached)).collect()
    }

    // the vehicle node is gone - its components with it
    pub fn release_all(&mut self) -> Vec<ComponentItem>
    {
        self.sounds_mut().filter_map(|sound| std::mem::take(sound).as_ref().cloned()).collect()
    }
}

fn resolve_sound_component(sound: &mut OptionOrId<ComponentItem>, components: &[ComponentItem])
{
    let Some(uuid) = sound.id().map(str::to_string) else { return; };

    let found = components.iter().find(|component|
    {
        let component = component.read().unwrap();
        component.get_base().uuid == uuid && component.as_any().is::<Sound>()
    });

    match found
    {
        Some(component) => *sound = OptionOrId::Some(component.clone()),
        None => { console_warning!("vehicle sound: no sound component with the uuid {}", uuid); },
    }
}

// the component was deleted or left the vehicle node - the reference goes, the component is handed back to be silenced
fn release_if_detached(sound: &mut OptionOrId<ComponentItem>, attached: &[ComponentItem]) -> Option<ComponentItem>
{
    let component = sound.as_ref()?.clone();
    let gone = component.read().unwrap().get_base().delete_later_request || !attached.iter().any(|item| Arc::ptr_eq(item, &component));

    if !gone
    {
        return None;
    }

    *sound = OptionOrId::None;
    Some(component)
}

// Equal gain crossfade over layers sorted by rpm - the weights of the two layers around the rpm, crossfaded around the middle of their log rpm step.
pub fn layer_weights(layer_rpms: &[f32], rpm: f32) -> Vec<f32>
{
    let mut weights = vec![0.0; layer_rpms.len()];

    if layer_rpms.is_empty()
    {
        return weights;
    }

    let mut order: Vec<usize> = (0..layer_rpms.len()).collect();
    order.sort_by(|a, b| layer_rpms[*a].total_cmp(&layer_rpms[*b]));

    let first = order[0];
    let last = order[order.len() - 1];

    if rpm <= layer_rpms[first]
    {
        weights[first] = 1.0;
        return weights;
    }

    if rpm >= layer_rpms[last]
    {
        weights[last] = 1.0;
        return weights;
    }

    for pair in order.windows(2)
    {
        let (low, high) = (pair[0], pair[1]);

        if rpm >= layer_rpms[low] && rpm <= layer_rpms[high]
        {
            let (from, to) = (layer_rpms[low].max(1.0), layer_rpms[high].max(1.0));
            let t = (rpm.max(from) / from).ln() / (to / from).ln().max(0.001);
            let t = ((t - 0.5) / CROSSFADE_WIDTH + 0.5).clamp(0.0, 1.0);

            // equal power (the weights' squares sum to 1) was up to +3 dB loud in the middle - loops cut from one recording add up like one signal
            // weights[low] = (t * PI * 0.5).cos();
            // weights[high] = (t * PI * 0.5).sin();

            // equal gain: the weights sum to 1
            weights[low] = (t * PI * 0.5).cos().powi(2);
            weights[high] = (t * PI * 0.5).sin().powi(2);
            break;
        }
    }

    weights
}

// The rpm crossfade inside the on and the off throttle group, the groups blended by the engine load. A group
// without layers hands its share to the other one.
pub fn load_layer_weights(layer_rpms: &[f32], layer_loads: &[f32], rpm: f32, engine_load: f32) -> Vec<f32>
{
    let on: Vec<usize> = (0..layer_rpms.len()).filter(|i| layer_loads[*i] >= 0.5).collect();
    let off: Vec<usize> = (0..layer_rpms.len()).filter(|i| layer_loads[*i] < 0.5).collect();

    let engine_load = engine_load.clamp(0.0, 1.0);
    let (on_share, off_share) = match (on.is_empty(), off.is_empty())
    {
        // equal power, see layer_weights
        // (false, false) => ((engine_load * PI * 0.5).sin().max(0.0), (engine_load * PI * 0.5).cos().max(0.0)),
        (false, false) => ((engine_load * PI * 0.5).sin().powi(2), (engine_load * PI * 0.5).cos().powi(2)),
        (false, true) => (1.0, 0.0),
        (true, false) => (0.0, 1.0),
        (true, true) => (0.0, 0.0),
    };

    let mut weights = vec![0.0; layer_rpms.len()];

    for (group, share) in [(on, on_share), (off, off_share)]
    {
        let rpms: Vec<f32> = group.iter().map(|i| layer_rpms[*i]).collect();

        for (index, weight) in group.iter().zip(layer_weights(&rpms, rpm))
        {
            weights[*index] = weight * share;
        }
    }

    weights
}

// What the controller hands over every frame.
pub struct VehicleSoundInput
{
    pub rpm: f32,
    pub engine_load: f32, // 0..1, how hard the engine works - the smoothed throttle
    pub squeal: f32, // 0..1, tires sliding sideways or locked
    pub speed: f32, // m/s
    pub limiter: bool,
}

// Drives the gain and pitch of the assigned sound components. The components play and position themselves as part of the node.
pub struct VehicleSoundPlayer
{
    playing: Vec<ComponentItem>, // started by the player - silenced once they are no longer assigned

    squeal_level: f32,
    smoothed_engine_load: f32,

    random_state: u32, // seeded per vehicle, so the wobble sounds the same every run
    wobble: f32,
    wobble_target: f32,
    wobble_timer: f32,
    lfo_phase: f32,
}

impl VehicleSoundPlayer
{
    pub fn new(seed: u32) -> Self
    {
        Self
        {
            playing: vec![],

            squeal_level: 0.0,
            smoothed_engine_load: 0.0,

            random_state: seed | 1, // xorshift stays at 0 forever
            wobble: 0.0,
            wobble_target: 0.0,
            wobble_timer: 0.0,
            lfo_phase: (seed % 628) as f32 * 0.01,
        }
    }

    // pseudo random -1..1 (xorshift32) - no need for more here
    fn random_signed(&mut self) -> f32
    {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 17;
        self.random_state ^= self.random_state << 5;

        (self.random_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    pub fn update(&mut self, settings: &VehicleSoundSettings, input: &VehicleSoundInput, dt: f32)
    {
        // ********** wobble **********
        // a random walk plus a slow sine, stronger under load - a real engine is never perfectly steady
        self.wobble_timer -= dt;
        if self.wobble_timer <= 0.0
        {
            let interval = WOBBLE_MIN_INTERVAL + (self.random_signed() * 0.5 + 0.5) * (WOBBLE_MAX_INTERVAL - WOBBLE_MIN_INTERVAL);
            self.wobble_timer = interval;
            self.wobble_target = self.random_signed();
        }

        self.wobble += (self.wobble_target - self.wobble) * (1.0 - (-WOBBLE_SMOOTHING * dt).exp());
        self.lfo_phase = (self.lfo_phase + WOBBLE_LFO_SPEED * dt) % (PI * 2.0);

        // the throttle jumps, the sound of it does not
        self.smoothed_engine_load += (input.engine_load.clamp(0.0, 1.0) - self.smoothed_engine_load) * (1.0 - (-10.0 * dt).exp());
        let engine_load = self.smoothed_engine_load;

        let load_factor = 0.4 + 0.6 * engine_load;
        let wobble = settings.pitch_variation * load_factor * (0.65 * self.wobble + 0.35 * self.lfo_phase.sin());

        let rpm = input.rpm.max(0.0) * (1.0 + wobble);

        let mut active: Vec<ComponentItem> = vec![];

        // ********** engine layers **********
        let rpms: Vec<f32> = settings.engine_layers.iter().map(|layer| layer.rpm.max(1.0)).collect();
        let loads: Vec<f32> = settings.engine_layers.iter().map(|layer| layer.load).collect();
        let weights = load_layer_weights(&rpms, &loads, rpm, engine_load);

        // with overrun loops the engine load is already audible in the files, otherwise the volume carries it
        let has_off_layers = loads.iter().any(|layer_load| *layer_load < 0.5);
        let mut loudness = if has_off_layers { 0.8 + 0.2 * engine_load } else { 0.55 + 0.45 * engine_load } * (1.0 + wobble * 3.0);

        // engines without idle (electric, pedals) fall silent towards a standstill instead of droning on at the lowest pitch
        if let Some(lowest) = rpms.iter().copied().reduce(f32::min)
        {
            loudness *= (rpm / (lowest * MIN_PITCH)).clamp(0.0, 1.0);
        }

        if input.limiter
        {
            loudness *= 0.8;
        }

        for ((layer, layer_rpm), weight) in settings.engine_layers.iter().zip(rpms.iter()).zip(weights.iter())
        {
            let Some(component) = layer.sound.as_ref() else { continue; };

            let pitch = (rpm / layer_rpm).clamp(MIN_PITCH, MAX_PITCH);
            Self::drive(component, (loudness * weight).max(0.0), pitch);
            active.push(component.clone());
        }

        // ********** tires **********
        let target = input.squeal.clamp(0.0, 1.0);
        let rate = if target > self.squeal_level { TIRE_ATTACK } else { TIRE_RELEASE };
        self.squeal_level += (target - self.squeal_level) * (1.0 - (-rate * dt).exp());

        let speed = input.speed.abs();

        if let Some(component) = settings.squeal.as_ref()
        {
            // a harder slide squeals a little higher - the loops are recordings, pitched far they sound slowed down
            Self::drive(component, self.squeal_level, 0.92 + 0.16 * self.squeal_level);
            active.push(component.clone());
        }

        if let Some(component) = settings.road.as_ref()
        {
            let share = (speed / ROAD_FULL_SPEED).clamp(0.0, 1.2);
            Self::drive(component, share, 0.6 + 0.6 * share.min(1.0));
            active.push(component.clone());
        }

        // ********** no longer assigned **********
        for component in &self.playing
        {
            if !active.iter().any(|item| Arc::ptr_eq(item, component))
            {
                Self::silence(component);
            }
        }

        self.playing = active;
    }

    fn drive(component: &ComponentItem, gain: f32, pitch: f32)
    {
        component_downcast_mut!(component, Sound);

        if !component.is_enabled()
        {
            if component.running()
            {
                component.stop();
            }
            return;
        }

        component.set_runtime_modulation(gain, pitch);

        if !component.running()
        {
            component.start();
        }
    }

    // stopped and back to the volume and speed set on the component
    pub fn silence(component: &ComponentItem)
    {
        component_downcast_mut!(component, Sound);
        component.stop();
        component.set_runtime_modulation(1.0, 1.0);
    }

    pub fn stop(&mut self)
    {
        for component in self.playing.drain(..)
        {
            Self::silence(&component);
        }
    }
}
