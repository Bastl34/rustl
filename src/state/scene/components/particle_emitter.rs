use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};

use nalgebra::{Matrix4, Point3, Vector3, Vector4};
use egui::RichText;
use rapier3d::prelude::{QueryFilter, Ray, Vector};
use serde::{Deserialize, Serialize};

use crate::{component_impl_default, component_impl_no_cleanup_node, console_error, gui::helper::property_items::{color_edit, combo, enum_combo, vector_edit}, helper::{change_tracker::ChangeTracker, easing::{easing, Easing}, option_or_id::OptionOrId}, state::{resources::texture::TextureItem, scene::{exporter::serialization_helper::{deserialize_texture, serialize_texture}, components::{component::ComponentItem, mesh::Mesh, sound::{Sound, SoundType}, transformation::Transformation}, node::{InstanceItemArc, NodeItem}, physics::physics_world::PhysicsWorld, scene::Scene}, state::{get_delta_t, InputOutput}}};

use super::component::{ComponentBase, Component};
use crate::state::state::RunMode;
use crate::helper::math::{direction_in_cone, xorshift32};

// m off the surface after a bounce
const BOUNCE_LIFT: f32 = 0.005;

// m per update - a node moving more jumped (teleport)
const TELEPORT_DISTANCE: f32 = 50.0;

#[derive(PartialEq, Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub enum ParticleBlend
{
    #[default]
    Alpha,
    Additive,
}

// where new particles start, in node space
#[derive(PartialEq, Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub enum ParticleShape
{
    #[default]
    Point,
    Sphere { radius: f32 },
    Box { half_extents: Vector3<f32> },
    Circle { radius: f32, #[serde(default)] edge: bool }, // on the x/z plane
    Line { length: f32 }, // along x
    Mesh { #[serde(default)] along_normal: bool }, // surface of the Mesh component of the node
}

// how the quad of a particle is turned
#[derive(PartialEq, Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub enum ParticleAlign
{
    #[default]
    Billboard, // to the camera
    Velocity, // stretched along its way (sparks, rain)
    Node, // flat in the x/z plane of the node (shockwave on the ground)
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum ParticleEventKind
{
    Death,
    Hit,
}

// world space
#[derive(Clone, Copy, Debug)]
pub struct ParticleEvent
{
    pub kind: ParticleEventKind,
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub normal: Vector3<f32>, // zero for a death
}

// missing fields fall back to the defaults - older saves keep loading when fields are added
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleEmitterData
{
    // emission
    pub auto_play: bool,
    pub looped: bool,
    pub duration: f32, // s, one cycle
    pub rate: f32, // particles per s
    pub burst: u32, // at the start of every cycle
    pub max_particles: u32,
    pub world_space: bool,

    // spawn
    pub shape: ParticleShape,
    pub direction: Vector3<f32>, // node space
    pub cone_angle: f32, // deg
    pub speed: f32,
    pub speed_var: f32,
    pub life: f32,
    pub life_var: f32,

    // motion
    pub gravity_factor: f32,
    pub drag: f32, // share of the speed lost per s
    pub spin: f32, // deg/s
    pub spin_var: f32,
    pub random_rotation: bool,
    pub noise_strength: f32, // m/s² of the turbulence
    pub noise_frequency: f32, // 1/m
    pub noise_speed: f32, // how fast the turbulence changes
    pub inherit_velocity: f32, // share of the node velocity new particles take over (world space)
    pub vortex: f32, // m/s² around the y axis of the node (tornado, portal, black hole)
    pub attraction: f32, // m/s² toward the y axis of the node, negative pushes away
    pub attract_to_center: bool, // toward the node origin instead of its y axis
    pub absorb_radius: f32, // m - particles closer to the axis (center) die, 0: off
    pub spread_spawns: bool, // a moving node: the spawns of a frame lie along its way instead of in a clump

    // look
    pub size_start: f32,
    pub size_end: f32,
    pub size_var: f32,
    pub size_easing: Easing,
    pub color_start: Vector4<f32>,
    pub color_end: Vector4<f32>,
    pub color_easing: Easing,
    pub random_hue: f32, // 0..1: each particle gets its color turned by up to this share of the color wheel (confetti)
    pub alpha_easing: Easing,
    pub fade_in: f32, // share of the life the alpha rises from 0
    pub intensity: f32, // for glow later - not used and not in the ui until then
    pub blend: ParticleBlend,
    pub sort: bool, // back to front per camera (alpha blend)
    pub align: ParticleAlign,
    pub stretch: f32, // s of the way drawn behind a particle (align Velocity)
    pub fog: bool,
    pub softness: f32, // m - fades out where the particle comes close to the scene behind it, 0: off
    pub trail: bool, // a ribbon behind every particle
    pub trail_length: f32, // s
    pub trail_width: f32, // share of the size
    pub cast_shadows: bool, // alpha tested quads in the shadow maps
    pub translucent_shadows: bool, // the alpha lets light through (rendering setting), else hard
    #[serde(serialize_with = "serialize_texture", deserialize_with = "deserialize_texture")]
    pub texture: OptionOrId<TextureItem>, // a texture of the resources, saved by uuid (the scene file lists the file)
    pub flipbook: [u32; 2], // columns, rows
    pub flipbook_fps: f32, // 0: once over the life

    // collision
    pub collide: bool,
    pub bounce: f32,
    pub friction: f32,
    pub die_on_hit: bool, // a hit ends the particle (rain drops) - it reports a hit and a death
    pub die_min_up: f32, // die_on_hit only on surfaces whose normal points up at least this much (y), steeper ones bounce - -1: every hit
    // events - sub emitters are other emitters of the node, by component name
    pub sub_emitter_death: String,
    pub sub_emitter_hit: String,
    pub sub_emitter_count: u32, // particles per event
    pub sub_emitter_inherit: f32, // share of the velocity the new particles take over
    pub report_events: bool, // events() for code

    // culling
    pub max_distance: f32, // m to the camera, 0: no limit
    pub pause_when_hidden: bool, // no simulation while no camera sees it
    pub skip_when_hidden: bool, // instead of the pause: the time goes on, no new particles while no camera sees it (an explosion off screen is over when looked at)

    // one shot effects (spawn_effect sets it): the node is removed once all of its emitters are done
    pub remove_when_done: bool,

    // sound - Sound components of the node by name: the loop plays while the emitter emits, the other one is played once per burst / sub emitter event
    pub sound_loop: String,
    pub sound_on_burst: String,
    pub sound_max_per_second: f32, // one shots (rain splashes) - more are dropped
}

impl Default for ParticleEmitterData
{
    fn default() -> Self
    {
        ParticleEmitterData
        {
            auto_play: true,
            looped: true,
            duration: 5.0,
            rate: 20.0,
            burst: 0,
            max_particles: 1000,
            world_space: true,

            shape: ParticleShape::Point,
            direction: Vector3::new(0.0, 1.0, 0.0),
            cone_angle: 25.0,
            speed: 2.0,
            speed_var: 0.5,
            life: 2.0,
            life_var: 0.5,

            gravity_factor: 0.0,
            drag: 0.0,
            spin: 0.0,
            spin_var: 0.0,
            random_rotation: true,
            noise_strength: 0.0,
            noise_frequency: 0.5,
            noise_speed: 0.5,
            inherit_velocity: 0.0,
            vortex: 0.0,
            attraction: 0.0,
            attract_to_center: false,
            absorb_radius: 0.0,
            spread_spawns: true,

            size_start: 0.2,
            size_end: 0.5,
            size_var: 0.05,
            size_easing: Easing::None,
            color_start: Vector4::new(1.0, 1.0, 1.0, 1.0),
            color_end: Vector4::new(1.0, 1.0, 1.0, 0.0),
            color_easing: Easing::None,
            random_hue: 0.0,
            alpha_easing: Easing::None,
            fade_in: 0.0,
            intensity: 1.0,
            blend: ParticleBlend::Alpha,
            sort: false,
            align: ParticleAlign::Billboard,
            stretch: 0.05,
            fog: true,
            softness: 0.0,
            trail: false,
            trail_length: 0.3,
            trail_width: 1.0,
            cast_shadows: false,
            translucent_shadows: false,
            texture: OptionOrId::None,
            flipbook: [1, 1],
            flipbook_fps: 0.0,

            collide: false,
            bounce: 0.3,
            friction: 0.3,
            die_on_hit: false,
            die_min_up: -1.0,

            sub_emitter_death: String::new(),
            sub_emitter_hit: String::new(),
            sub_emitter_count: 5,
            sub_emitter_inherit: 0.3,
            report_events: false,

            max_distance: 0.0,
            pause_when_hidden: true,
            skip_when_hidden: false,

            remove_when_done: false,

            sound_loop: String::new(),
            sound_on_burst: String::new(),
            sound_max_per_second: 10.0,
        }
    }
}

// in world space or node space (world_space off)
#[derive(Clone, Debug)]
pub struct Particle
{
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub age: f32, // s
    pub life: f32, // s
    pub size_offset: f32, // added to the size over the life
    pub rotation: f32, // rad
    pub spin: f32, // rad/s
    pub hue_shift: f32, // rad on the color wheel (random_hue)
    pub trail: Vec<(Vector3<f32>, f32)>, // older positions and the age there, oldest first (trail on)
}

impl Particle
{
    // 0..1 over the life
    pub fn progress(&self) -> f32
    {
        (self.age / self.life).clamp(0.0, 1.0)
    }
}

// triangles of the mesh of the node, picked by area
struct MeshSampler
{
    key: (u32, usize), // mesh resource id, triangle count
    triangles: Vec<[Point3<f32>; 3]>,
    cumulative_area: Vec<f32>,
}

// not saved
#[derive(Default)]
struct ParticleRuntime
{
    particles: Vec<Particle>,
    started: bool,
    playing: bool,
    cycle_time: f32,
    spawn_acc: f32,
    pending: u32,
    requests: Vec<(Vector3<f32>, Vector3<f32>, u32)>, // world position, world velocity, count - from emit_at
    events: Vec<ParticleEvent>, // of the last update
    time: f32,
    random: u32,
    hidden: AtomicBool, // set by the renderer
    mesh: Option<MeshSampler>,
    texture_choices: Vec<TextureItem>, // for the texture field - set by the editor
    last_origin: Option<Vector3<f32>>, // world position of the node in the last update
    burst_positions: Vec<Vector3<f32>>, // world positions of the bursts of the last update (sound)
    one_shots: Vec<Sound>,
    sounds_played: f32, // one shots of about the last second (decays) - the limit of sound_max_per_second
}

#[derive(Serialize, Deserialize)]
pub struct ParticleEmitter
{
    base: ComponentBase,
    data: ChangeTracker<ParticleEmitterData>,

    #[serde(skip, default)]
    runtime: ParticleRuntime,
}

impl ParticleEmitter
{
    pub fn new(name: &str) -> ParticleEmitter
    {
        let emitter = ParticleEmitter
        {
            base: ComponentBase::new(name.to_string(), "Particle Emitter".to_string(), "✨".to_string()),
            data: ChangeTracker::new(ParticleEmitterData::default()),
            runtime: ParticleRuntime::default(),
        };

        emitter
    }

    pub fn new_empty(name: &str) -> ParticleEmitter
    {
        ParticleEmitter::new(name)
    }

    pub fn get_data(&self) -> &ParticleEmitterData
    {
        &self.data.get_ref()
    }

    pub fn get_data_tracker(&self) -> &ChangeTracker<ParticleEmitterData>
    {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut ChangeTracker<ParticleEmitterData>
    {
        &mut self.data
    }

    // ********** control **********

    // starts a new cycle with its burst
    pub fn play(&mut self)
    {
        self.runtime.started = true;
        self.runtime.playing = true;
        self.runtime.cycle_time = 0.0;
        self.runtime.spawn_acc = 0.0;
        self.runtime.pending += self.data.get_ref().burst;
    }

    // no new particles - the living ones fade out
    pub fn stop(&mut self)
    {
        self.runtime.started = true;
        self.runtime.playing = false;
        self.runtime.pending = 0;
    }

    pub fn clear(&mut self)
    {
        self.runtime.particles.clear();
    }

    // back to the state before the first update - leaving simulate / play
    fn reset_runtime(&mut self)
    {
        let old = std::mem::take(&mut self.runtime);

        for mut one_shot in old.one_shots
        {
            one_shot.stop();
        }

        self.runtime.random = old.random;
        self.runtime.hidden = old.hidden;
        self.runtime.mesh = old.mesh;
        self.runtime.texture_choices = old.texture_choices;
    }

    // spawned in the next update, also while stopped
    pub fn burst(&mut self, count: u32)
    {
        self.runtime.pending += count;
    }

    // count particles at a world position, the velocity added to theirs - also while stopped (sub emitters)
    pub fn emit_at(&mut self, position: Vector3<f32>, velocity: Vector3<f32>, count: u32)
    {
        self.runtime.requests.push((position, velocity, count));
    }

    pub fn is_playing(&self) -> bool
    {
        self.runtime.playing
    }

    // nothing left to show and nothing more to come (an emitter without auto play waits for sub emitter events)
    pub fn is_done(&self) -> bool
    {
        (self.runtime.started || !self.data.get_ref().auto_play) && !self.runtime.playing && self.runtime.particles.is_empty() && self.runtime.requests.is_empty() && self.runtime.pending == 0
    }

    pub fn particles(&self) -> &[Particle]
    {
        &self.runtime.particles
    }

    // deaths and hits of the last update - only with report_events or a sub emitter
    pub fn events(&self) -> &[ParticleEvent]
    {
        &self.runtime.events
    }

    // set by the renderer: no camera sees it
    pub fn set_hidden(&self, hidden: bool)
    {
        self.runtime.hidden.store(hidden, Ordering::Relaxed);
    }

    // m from the node origin new particles can get in their life (shape + way)
    pub fn reach(&self) -> f32
    {
        let data = self.data.get_ref();
        let shape = match data.shape
        {
            ParticleShape::Point | ParticleShape::Mesh { .. } => 0.0,
            ParticleShape::Sphere { radius } | ParticleShape::Circle { radius, .. } => radius,
            ParticleShape::Box { half_extents } => half_extents.norm(),
            ParticleShape::Line { length } => length * 0.5,
        };

        shape + (data.speed + data.speed_var) * (data.life + data.life_var) + data.size_start.max(data.size_end) + data.size_var
    }

    // the textures the ui offers
    pub fn set_texture_choices(&mut self, choices: Vec<TextureItem>)
    {
        self.runtime.texture_choices = choices;
    }

    // ********** look of a particle **********

    pub fn particle_size(&self, particle: &Particle) -> f32
    {
        let data = self.data.get_ref();
        let t = easing(data.size_easing, particle.progress());
        (data.size_start + (data.size_end - data.size_start) * t + particle.size_offset).max(0.0)
    }

    // start to end over the life, the alpha with its own easing and the fade in
    pub fn particle_color(&self, particle: &Particle) -> Vector4<f32>
    {
        let data = self.data.get_ref();
        let progress = particle.progress();

        let color_t = easing(data.color_easing, progress);
        let alpha_t = easing(data.alpha_easing, progress);

        let mut color = data.color_start.lerp(&data.color_end, color_t);
        color.w = data.color_start.w + (data.color_end.w - data.color_start.w) * alpha_t;

        if data.fade_in > 0.0 && progress < data.fade_in
        {
            color.w *= progress / data.fade_in;
        }

        if particle.hue_shift != 0.0
        {
            let rgb = turn_hue(&color.xyz(), particle.hue_shift);
            color = Vector4::new(rgb.x, rgb.y, rgb.z, color.w);
        }

        color
    }

    // tile of the flipbook, row by row
    pub fn particle_frame(&self, particle: &Particle) -> u32
    {
        let data = self.data.get_ref();
        let frames = data.flipbook[0].max(1) * data.flipbook[1].max(1);
        let frame = if data.flipbook_fps > 0.0 { (particle.age * data.flipbook_fps) as u32 % frames } else { (particle.progress() * frames as f32) as u32 };
        frame.min(frames - 1)
    }

    // ********** simulation **********

    fn simulate(&mut self, transform: &Matrix4<f32>, physics: &PhysicsWorld, dt: f32)
    {
        self.runtime.events.clear();

        let data = self.data.get_ref().clone();

        // the way of the node since the last update - a jump (teleport) counts as standing still
        let origin = transform.column(3).xyz();
        let movement = self.runtime.last_origin.replace(origin).map(|last| origin - last).filter(|movement| movement.norm() < TELEPORT_DISTANCE).unwrap_or(Vector3::zeros());

        // spawn requests (sub emitter) are not paused: they come from a visible emitter and land elsewhere than the node of this one
        let hidden = self.runtime.hidden.load(Ordering::Relaxed);
        if data.pause_when_hidden && !data.skip_when_hidden && hidden && self.runtime.requests.is_empty()
        {
            return;
        }

        if dt <= 0.0
        {
            return;
        }

        self.runtime.time += dt;

        if !self.runtime.started
        {
            self.runtime.started = true;
            if data.auto_play
            {
                self.play();
            }
        }

        // emission (duration 0: no cycles)
        if self.runtime.playing
        {
            self.runtime.spawn_acc += data.rate * dt;
            self.runtime.cycle_time += dt;

            if data.duration > 0.0 && self.runtime.cycle_time >= data.duration
            {
                if data.looped
                {
                    self.runtime.cycle_time %= data.duration;
                    self.runtime.pending += data.burst;
                }
                else
                {
                    self.runtime.playing = false;
                    self.runtime.spawn_acc = 0.0;
                }
            }
        }

        self.runtime.burst_positions.clear();
        if self.runtime.pending > 0
        {
            self.runtime.burst_positions.push(origin);
        }

        // skipped: what would come now is dropped, the living particles go on
        if data.skip_when_hidden && hidden
        {
            self.runtime.pending = 0;
            self.runtime.spawn_acc = self.runtime.spawn_acc.fract();
            self.runtime.requests.clear();
        }

        let wanted = self.runtime.pending as usize + self.runtime.spawn_acc as usize;
        self.runtime.spawn_acc = self.runtime.spawn_acc.fract();
        self.runtime.pending = 0;

        let node_velocity = movement / dt;
        let mut free = (data.max_particles as usize).saturating_sub(self.runtime.particles.len());
        let count = wanted.min(free);
        for index in 0..count
        {
            let mut particle = self.spawn(&data, transform, None);

            if data.world_space
            {
                // spread over the way of this frame, the first one where the node was
                if data.spread_spawns
                {
                    particle.position -= movement * (1.0 - (index as f32 + 0.5) / count as f32);
                }
                particle.velocity += node_velocity * data.inherit_velocity;
            }

            self.runtime.particles.push(particle);
        }

        let inverse = transform.try_inverse().unwrap_or(Matrix4::identity());

        for (position, velocity, count) in std::mem::take(&mut self.runtime.requests)
        {
            self.runtime.burst_positions.push(position);

            free = (data.max_particles as usize).saturating_sub(self.runtime.particles.len());
            for _ in 0..(count as usize).min(free)
            {
                let particle = self.spawn(&data, transform, Some((&inverse, position, velocity)));
                self.runtime.particles.push(particle);
            }
        }

        // node space: the world gravity turned into it
        let gravity = physics.settings.gravity * data.gravity_factor;
        let gravity = if data.world_space { gravity } else { inverse.transform_vector(&gravity) };
        let damping = (1.0 - data.drag * dt).max(0.0);
        let noise_time = self.runtime.time * data.noise_speed;

        let collect_events = data.report_events || !data.sub_emitter_death.is_empty() || !data.sub_emitter_hit.is_empty();

        // vortex / attraction around the y axis of the node, in the space of the particles
        let forces = data.vortex != 0.0 || data.attraction != 0.0 || data.absorb_radius > 0.0;
        let (force_origin, force_axis) = if data.world_space { (origin, transform.transform_vector(&Vector3::y()).try_normalize(1e-6).unwrap_or(Vector3::y())) } else { (Vector3::zeros(), Vector3::y()) };
        let mut events = vec![];
        let to_world = |position: Vector3<f32>, velocity: Vector3<f32>| if data.world_space { (position, velocity) } else { (transform.transform_point(&Point3::from(position)).coords, transform.transform_vector(&velocity)) };

        self.runtime.particles.retain_mut(|particle|
        {
            particle.age += dt;
            if particle.age >= particle.life
            {
                if collect_events
                {
                    let (position, velocity) = to_world(particle.position, particle.velocity);
                    events.push(ParticleEvent { kind: ParticleEventKind::Death, position, velocity, normal: Vector3::zeros() });
                }
                return false;
            }

            if forces
            {
                let offset = particle.position - force_origin;
                let radial = if data.attract_to_center { offset } else { offset - force_axis * offset.dot(&force_axis) };
                let distance = radial.norm();

                if distance < data.absorb_radius
                {
                    if collect_events
                    {
                        let (position, velocity) = to_world(particle.position, particle.velocity);
                        events.push(ParticleEvent { kind: ParticleEventKind::Death, position, velocity, normal: Vector3::zeros() });
                    }
                    return false;
                }

                if distance > 1e-4
                {
                    let inward = -radial / distance;
                    particle.velocity += (inward * data.attraction + force_axis.cross(&inward) * data.vortex) * dt;
                }
            }

            particle.velocity += gravity * dt;
            if data.noise_strength > 0.0
            {
                particle.velocity += turbulence(&particle.position, data.noise_frequency, noise_time) * data.noise_strength * dt;
            }
            particle.velocity *= damping;

            let way = particle.velocity * dt;

            let hit = if data.collide { cast(physics, transform, &inverse, data.world_space, particle.position, way) } else { None };
            match hit
            {
                Some((point, normal)) =>
                {
                    particle.position = point + normal * BOUNCE_LIFT;
                    let into = particle.velocity.dot(&normal);
                    if into < 0.0
                    {
                        let dies = data.die_on_hit && normal.y >= data.die_min_up;
                        if collect_events
                        {
                            let (position, velocity) = to_world(particle.position, particle.velocity);
                            let (_, normal) = to_world(Vector3::zeros(), normal);
                            events.push(ParticleEvent { kind: ParticleEventKind::Hit, position, velocity, normal: normal.try_normalize(1e-6).unwrap_or(normal) });

                            if dies
                            {
                                events.push(ParticleEvent { kind: ParticleEventKind::Death, position, velocity, normal: Vector3::zeros() });
                            }
                        }

                        if dies
                        {
                            return false;
                        }

                        let along = particle.velocity - normal * into;
                        particle.velocity = along * (1.0 - data.friction) - normal * into * data.bounce;
                    }
                },
                None => particle.position += way,
            }

            particle.rotation += particle.spin * dt;

            // a point of the trail where the particle moved far enough, the old end gets cut off
            if data.trail
            {
                let spacing = (data.size_start.max(data.size_end) * data.trail_width * 0.3).max(0.02);
                if particle.trail.last().is_none_or(|(last, _)| (particle.position - last).norm() >= spacing)
                {
                    particle.trail.push((particle.position, particle.age));
                }

                let cut = particle.trail.iter().take_while(|(_, age)| particle.age - age > data.trail_length).count();
                particle.trail.drain(..cut);
            }

            true
        });

        self.runtime.events = events;
    }

    // origin: a world position and velocity to start from (emit_at) instead of the node
    fn spawn(&mut self, data: &ParticleEmitterData, transform: &Matrix4<f32>, origin: Option<(&Matrix4<f32>, Vector3<f32>, Vector3<f32>)>) -> Particle
    {
        let mut normal = None;
        let offset = match data.shape
        {
            ParticleShape::Point => Vector3::zeros(),
            ParticleShape::Sphere { radius } => self.random_direction(std::f32::consts::PI, &Vector3::y()) * radius * self.random().cbrt(),
            ParticleShape::Box { half_extents } => Vector3::new(self.random_signed() * half_extents.x, self.random_signed() * half_extents.y, self.random_signed() * half_extents.z),
            ParticleShape::Circle { radius, edge } =>
            {
                let angle = self.random() * std::f32::consts::TAU;
                let distance = if edge { radius } else { radius * self.random().sqrt() };
                Vector3::new(angle.cos() * distance, 0.0, angle.sin() * distance)
            },
            ParticleShape::Line { length } => Vector3::new(self.random_signed() * length * 0.5, 0.0, 0.0),
            ParticleShape::Mesh { along_normal } =>
            {
                let (point, mesh_normal) = self.random_mesh_point().unwrap_or((Vector3::zeros(), Vector3::y()));
                if along_normal { normal = Some(mesh_normal); }
                point
            },
        };

        let axis = normal.or(data.direction.try_normalize(1e-6)).unwrap_or(Vector3::y());
        let direction = self.random_direction(data.cone_angle.to_radians(), &axis);
        let speed = (data.speed + self.random_signed() * data.speed_var).max(0.0);

        let (position, velocity) = match (data.world_space, origin)
        {
            (true, None) =>
            {
                let direction = transform.transform_vector(&direction).try_normalize(1e-6).unwrap_or(direction);
                (transform.transform_point(&Point3::from(offset)).coords, direction * speed)
            },
            (false, None) => (offset, direction * speed),
            (true, Some((_, position, velocity))) =>
            {
                let direction = transform.transform_vector(&direction).try_normalize(1e-6).unwrap_or(direction);
                (position + transform.transform_vector(&offset), direction * speed + velocity)
            },
            (false, Some((inverse, position, velocity))) => (inverse.transform_point(&Point3::from(position)).coords + offset, direction * speed + inverse.transform_vector(&velocity)),
        };

        Particle
        {
            position,
            velocity,
            age: 0.0,
            life: (data.life + self.random_signed() * data.life_var).max(0.01),
            size_offset: self.random_signed() * data.size_var,
            rotation: if data.random_rotation { self.random() * std::f32::consts::TAU } else { 0.0 },
            spin: (data.spin + self.random_signed() * data.spin_var).to_radians(),
            hue_shift: self.random_signed() * data.random_hue * std::f32::consts::PI,
            trail: vec![],
        }
    }

    // a point on the mesh (even by area) and the normal of its triangle, node space
    fn random_mesh_point(&mut self) -> Option<(Vector3<f32>, Vector3<f32>)>
    {
        let total = *self.runtime.mesh.as_ref()?.cumulative_area.last()?;
        let pick = self.random() * total;
        let (u, v) = (self.random(), self.random());

        let mesh = self.runtime.mesh.as_ref()?;
        let index = mesh.cumulative_area.partition_point(|area| *area < pick).min(mesh.triangles.len() - 1);
        let [a, b, c] = mesh.triangles[index];

        // even on the triangle
        let su = u.sqrt();
        let point = a.coords * (1.0 - su) + b.coords * (su * (1.0 - v)) + c.coords * (su * v);
        let normal = (b - a).cross(&(c - a)).try_normalize(1e-9).unwrap_or(Vector3::y());

        Some((point, normal))
    }

    // the triangles of the Mesh component of the node - only built again when the mesh changes
    fn refresh_mesh(&mut self, node: &NodeItem)
    {
        let Some(mesh_component) = node.read().unwrap().find_component::<Mesh>() else { self.runtime.mesh = None; return; };
        let mesh_component = mesh_component.read().unwrap();
        let Some(mesh) = mesh_component.as_any().downcast_ref::<Mesh>() else { return; };
        let Some(resource) = mesh.mesh_resource.as_ref() else { self.runtime.mesh = None; return; };

        let resource = resource.read().unwrap();
        let mesh_data = resource.get_data();
        let key = (resource.id, mesh_data.indices.len());

        if self.runtime.mesh.as_ref().is_some_and(|mesh| mesh.key == key)
        {
            return;
        }

        let mut triangles = Vec::with_capacity(mesh_data.indices.len());
        let mut cumulative_area = Vec::with_capacity(mesh_data.indices.len());
        let mut total = 0.0;

        for [a, b, c] in &mesh_data.indices
        {
            let (Some(a), Some(b), Some(c)) = (mesh_data.vertices.get(*a as usize), mesh_data.vertices.get(*b as usize), mesh_data.vertices.get(*c as usize)) else { continue; };

            total += (b - a).cross(&(c - a)).norm() * 0.5;
            triangles.push([*a, *b, *c]);
            cumulative_area.push(total);
        }

        self.runtime.mesh = if triangles.is_empty() { None } else { Some(MeshSampler { key, triangles, cumulative_area }) };
    }

    // the loop runs while the emitter emits, a one shot per burst position (limited per second, a bit of pitch variation)
    fn update_sounds(&mut self, node: &NodeItem, dt: f32)
    {
        let data = self.data.get_ref().clone();

        for component in node.read().unwrap().find_components::<Sound>()
        {
            let mut component = component.write().unwrap();
            let name = component.get_base().name.clone();
            let Some(sound) = component.as_any_mut().downcast_mut::<Sound>() else { continue; };

            if name == data.sound_loop
            {
                if self.runtime.playing && !sound.running()
                {
                    sound.start();
                }
                else if !self.runtime.playing && sound.running()
                {
                    sound.stop();
                }
            }

            if name == data.sound_on_burst && !self.runtime.burst_positions.is_empty()
            {
                let Some(source) = sound.sound_source.as_ref().cloned() else { continue; };
                let template = sound.get_data().clone();

                for position in std::mem::take(&mut self.runtime.burst_positions)
                {
                    if self.runtime.sounds_played >= data.sound_max_per_second.max(1.0)
                    {
                        break;
                    }
                    self.runtime.sounds_played += 1.0;

                    let pitch = 1.0 + self.random_signed() * 0.08;
                    let mut one_shot = Sound::new(&name, source.clone(), SoundType::Spatial, false);
                    {
                        let one_shot_data = one_shot.get_data_mut().get_mut();
                        one_shot_data.volume = template.volume;
                        one_shot_data.speed = template.speed * pitch;
                        one_shot_data.spatial_distance_scale = template.spatial_distance_scale;
                    }
                    one_shot.set_fixed_position(Some(Point3::from(position)));
                    one_shot.update_state(None, None, true);
                    one_shot.start();
                    self.runtime.one_shots.push(one_shot);
                }
            }
        }

        self.runtime.sounds_played = (self.runtime.sounds_played - dt * data.sound_max_per_second).max(0.0);

        // the spatial ones follow the listener
        for one_shot in self.runtime.one_shots.iter_mut()
        {
            one_shot.update_state(None, None, false);
        }
        self.runtime.one_shots.retain(|one_shot| !one_shot.stopped());
    }

    // the events to the sub emitters of the node
    fn feed_sub_emitters(&self, node: &NodeItem)
    {
        let data = self.data.get_ref();

        for (name, kind) in [(&data.sub_emitter_death, ParticleEventKind::Death), (&data.sub_emitter_hit, ParticleEventKind::Hit)]
        {
            if name.is_empty()
            {
                continue;
            }

            let points: Vec<(Vector3<f32>, Vector3<f32>)> = self.runtime.events.iter().filter(|event| event.kind == kind).map(|event| (event.position, event.velocity * data.sub_emitter_inherit)).collect();
            if points.is_empty()
            {
                continue;
            }

            // this emitter is not in the list of the node while it updates
            for component in node.read().unwrap().find_components::<ParticleEmitter>()
            {
                let mut component = component.write().unwrap();
                if component.get_base().name != *name
                {
                    continue;
                }

                if let Some(sub) = component.as_any_mut().downcast_mut::<ParticleEmitter>()
                {
                    for (position, velocity) in &points
                    {
                        sub.emit_at(*position, *velocity, data.sub_emitter_count);
                    }
                }
            }
        }
    }

    // even over the cap of this half angle around the axis
    fn random_direction(&mut self, angle: f32, axis: &Vector3<f32>) -> Vector3<f32>
    {
        let (rand_a, rand_b) = (self.random(), self.random());
        direction_in_cone(axis, angle, rand_a, rand_b)
    }

    // 0..1 (xorshift32)
    fn random(&mut self) -> f32
    {
        if self.runtime.random == 0
        {
            self.runtime.random = self.base.id.wrapping_mul(2654435761) | 1; // xorshift stays at 0 forever
        }

        xorshift32(&mut self.runtime.random)
    }

    fn random_signed(&mut self) -> f32
    {
        self.random() * 2.0 - 1.0
    }
}

// a copy of the emitters of the template node at a world position (one shot effects from code) - they play once, the node removes itself when all are done
// the emitters of the template are best disabled (they stay idle), the copies are enabled - sub emitters keep finding each other by name
pub fn spawn_effect(scene: &mut Scene, template: &NodeItem, position: Point3<f32>) -> NodeItem
{
    let (name, emitters, sounds): (String, Vec<ComponentItem>, Vec<ComponentItem>) =
    {
        let template = template.read().unwrap();
        (template.name.clone(), template.find_components::<ParticleEmitter>(), template.find_components::<Sound>())
    };

    let node = scene.add_empty_node(&format!("{} (effect)", name), None);

    {
        let mut node = node.write().unwrap();
        node.settings.transient = true;

        let mut transform = Transformation::identity("Transform");
        transform.set_translation(position.coords);
        node.add_component(Arc::new(RwLock::new(Box::new(transform))));

        for emitter in emitters
        {
            let Some(copy) = emitter.read().unwrap().duplicate() else { continue; };

            {
                let mut copy = copy.write().unwrap();
                copy.set_enabled(true);
                if let Some(copy) = copy.as_any_mut().downcast_mut::<ParticleEmitter>()
                {
                    let data = copy.get_data_mut().get_mut();
                    data.looped = false;
                    data.remove_when_done = true;
                }
            }

            node.add_component(copy);
        }

        // started by the emitters (sound_loop, sound_on_burst)
        for sound in sounds
        {
            if let Some(copy) = sound.read().unwrap().duplicate()
            {
                node.add_component(copy);
            }
        }
    }

    node
}

// stops all emitters of the node - their particles live out, a spawned effect then removes itself (remove_when_done)
pub fn stop_effect(node: &NodeItem)
{
    for component in node.read().unwrap().find_components::<ParticleEmitter>()
    {
        let mut component = component.write().unwrap();
        if let Some(emitter) = component.as_any_mut().downcast_mut::<ParticleEmitter>()
        {
            emitter.stop();
        }
    }
}

// the hue turned by angle rad (rotation around the grey axis)
fn turn_hue(rgb: &Vector3<f32>, angle: f32) -> Vector3<f32>
{
    let axis = Vector3::new(1.0, 1.0, 1.0).normalize();
    let (sin, cos) = angle.sin_cos();
    let turned = rgb * cos + axis.cross(rgb) * sin + axis * axis.dot(rgb) * (1.0 - cos);
    turned.map(|value| value.max(0.0))
}

// a smooth swirling field, about -1..1 per axis
fn turbulence(position: &Vector3<f32>, frequency: f32, time: f32) -> Vector3<f32>
{
    let p = position * frequency;

    Vector3::new
    (
        (p.y * 1.7 + time * 1.3).sin() + (p.z * 2.3 - time * 0.7).sin() * 0.5,
        (p.z * 1.9 + time * 1.1).sin() + (p.x * 2.1 + time * 0.9).sin() * 0.5,
        (p.x * 1.5 - time * 1.2).sin() + (p.y * 2.7 + time * 0.8).sin() * 0.5,
    ) / 1.5
}

// where the way of this step hits a collider, in the space of the particle
fn cast(physics: &PhysicsWorld, transform: &Matrix4<f32>, inverse: &Matrix4<f32>, world_space: bool, from: Vector3<f32>, way: Vector3<f32>) -> Option<(Vector3<f32>, Vector3<f32>)>
{
    let (world_from, world_way) = if world_space { (from, way) } else { (transform.transform_point(&Point3::from(from)).coords, transform.transform_vector(&way)) };

    let length = world_way.norm();
    if length < 1e-5
    {
        return None;
    }

    let direction = world_way / length;
    let ray = Ray::new(Vector::new(world_from.x, world_from.y, world_from.z), Vector::new(direction.x, direction.y, direction.z));
    let (_, hit) = physics.query_pipeline(QueryFilter::default().exclude_sensors()).cast_ray_and_get_normal(&ray, length, true)?;

    // starting on a surface (spawned on the ground) is no hit - its normal can face the way and send the particle into the ground
    if hit.time_of_impact <= 1e-4
    {
        return None;
    }

    let point = world_from + direction * hit.time_of_impact;
    let normal = Vector3::new(hit.normal.x, hit.normal.y, hit.normal.z);
    if normal.norm_squared() < 0.25
    {
        return None;
    }

    if world_space
    {
        return Some((point, normal));
    }

    // normals go back with the transposed matrix
    let local_normal = (transform.fixed_view::<3, 3>(0, 0).transpose() * normal).try_normalize(1e-6)?;
    Some((inverse.transform_point(&Point3::from(point)).coords, local_normal))
}

#[typetag::serde]
impl Component for ParticleEmitter
{
    component_impl_default!();
    component_impl_no_cleanup_node!();

    // only simulate / play - a pause freezes it
    fn runs_in_mode(&self, run_mode: RunMode) -> bool
    {
        run_mode.is_running()
    }

    fn on_run_mode_changed(&mut self, node: NodeItem, _old: RunMode, new: RunMode)
    {
        if !new.is_running()
        {
            self.reset_runtime();
            self.update_sounds(&node, 0.0);
        }
    }

    fn update(&mut self, node: NodeItem, _io: &mut InputOutput, physics: &PhysicsWorld, _time: u128, frame_scale: f32, _frame: u64)
    {
        if matches!(self.data.get_ref().shape, ParticleShape::Mesh { .. })
        {
            self.refresh_mesh(&node);
        }

        let transform = node.read().unwrap().get_full_transform();
        self.simulate(&transform, physics, get_delta_t(frame_scale));

        if !self.runtime.events.is_empty()
        {
            self.feed_sub_emitters(&node);
        }

        self.update_sounds(&node, get_delta_t(frame_scale));

        // a one shot effect is gone when all of its emitters are done - this one is not in the list of the node while it updates
        if self.data.get_ref().remove_when_done && self.is_done()
        {
            let others_done = node.read().unwrap().find_components::<ParticleEmitter>().iter().all(|component|
            {
                let component = component.read().unwrap();
                component.as_any().downcast_ref::<ParticleEmitter>().is_none_or(|other| !other.is_enabled() || other.is_done())
            });

            if others_done
            {
                node.write().unwrap().delete_later();
            }
        }
    }

    fn update_instance(&mut self, _node: Option<NodeItem>, _instance: &InstanceItemArc, _io: &mut InputOutput, _physics: &PhysicsWorld, _time: u128, _frame_scale: f32, _frame: u64)
    {
    }

    fn run_after_deserialize(&mut self, context: &mut crate::state::scene::components::component::DeserializationContext)
    {
        // not saved
        self.base.component_name = "Particle Emitter".to_string();
        self.base.icon = "✨".to_string();

        let data = self.data.get_mut();
        if let Some(uuid) = data.texture.id().map(str::to_string)
        {
            data.texture = match context.textures.iter().find(|texture| texture.read().unwrap().matches_uuid(&uuid))
            {
                Some(texture) => OptionOrId::Some(texture.clone()),
                None =>
                {
                    console_error!("particle emitter {}: texture {} not found", self.base.name, uuid);
                    OptionOrId::None
                },
            };
        }
    }

    fn saved_with_node(&self) -> bool
    {
        true
    }

    fn instantiable() -> bool
    {
        false
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
        let source = self.as_any().downcast_ref::<ParticleEmitter>();

        if source.is_none()
        {
            return None;
        }

        let source = source.unwrap();

        let mut emitter = ParticleEmitter
        {
            base: ComponentBase::duplicate(source.get_base()),

            data: ChangeTracker::new(source.get_data().clone()),
            runtime: ParticleRuntime::default(),
        };

        emitter.data.force_change();

        Some(Arc::new(RwLock::new(Box::new(emitter))))
    }

    fn ui(&mut self, ui: &mut egui::Ui, node: Option<NodeItem>)
    {
        ui.horizontal(|ui|
        {
            let mut playing = self.is_playing();
            if ui.toggle_value(&mut playing, RichText::new("⏵").size(20.0)).on_hover_text("play").clicked() { self.play(); }
            if ui.button(RichText::new("⏹").size(20.0)).on_hover_text("stop emitting").clicked() { self.stop(); }
            if ui.button(RichText::new("🗑").size(20.0)).on_hover_text("remove all particles").clicked() { self.clear(); }
            ui.label(format!("particles: {}", self.particles().len()));
        });

        let mut changed = false;
        let mut data = self.get_data().clone();

        ui.collapsing("Emission", |ui|
        {
            changed = ui.checkbox(&mut data.auto_play, "auto play").changed() || changed;
            changed = ui.checkbox(&mut data.looped, "looped").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.duration, 0.0..=60.0).text("duration (s)")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.rate, 0.0..=1000.0).logarithmic(true).text("rate (1/s)")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.burst, 0..=1000).text("burst")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.max_particles, 1..=100000).logarithmic(true).text("max particles")).changed() || changed;
            changed = ui.checkbox(&mut data.world_space, "world space").on_hover_text("off: particles move with the node").changed() || changed;
        });

        ui.collapsing("Spawn", |ui|
        {
            ui.horizontal(|ui|
            {
                ui.label("shape:");
                let shape_name = match data.shape
                {
                    ParticleShape::Point => "Point",
                    ParticleShape::Sphere { .. } => "Sphere",
                    ParticleShape::Box { .. } => "Box",
                    ParticleShape::Circle { .. } => "Circle",
                    ParticleShape::Line { .. } => "Line",
                    ParticleShape::Mesh { .. } => "Mesh",
                };

                egui::ComboBox::from_id_salt(ui.make_persistent_id("particle_shape")).selected_text(shape_name).show_ui(ui, |ui|
                {
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Point), "Point").clicked() { data.shape = ParticleShape::Point; changed = true; }
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Sphere { .. }), "Sphere").clicked() { data.shape = ParticleShape::Sphere { radius: 0.5 }; changed = true; }
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Box { .. }), "Box").clicked() { data.shape = ParticleShape::Box { half_extents: Vector3::new(0.5, 0.5, 0.5) }; changed = true; }
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Circle { .. }), "Circle").clicked() { data.shape = ParticleShape::Circle { radius: 0.5, edge: false }; changed = true; }
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Line { .. }), "Line").clicked() { data.shape = ParticleShape::Line { length: 1.0 }; changed = true; }
                    if ui.selectable_label(matches!(data.shape, ParticleShape::Mesh { .. }), "Mesh").clicked() { data.shape = ParticleShape::Mesh { along_normal: false }; changed = true; }
                });
            });

            match &mut data.shape
            {
                ParticleShape::Point => {},
                ParticleShape::Sphere { radius } =>
                {
                    changed = ui.add(egui::Slider::new(radius, 0.0..=10.0).text("radius")).changed() || changed;
                },
                ParticleShape::Box { half_extents } =>
                {
                    changed = vector_edit(ui, "half extents", "", half_extents) || changed;
                },
                ParticleShape::Circle { radius, edge } =>
                {
                    changed = ui.add(egui::Slider::new(radius, 0.0..=10.0).text("radius")).changed() || changed;
                    changed = ui.checkbox(edge, "only the edge").changed() || changed;
                },
                ParticleShape::Line { length } =>
                {
                    changed = ui.add(egui::Slider::new(length, 0.0..=20.0).text("length (along x)")).changed() || changed;
                },
                ParticleShape::Mesh { along_normal } =>
                {
                    changed = ui.checkbox(along_normal, "along the surface normal").on_hover_text("instead of the direction below").changed() || changed;
                    if self.runtime.mesh.is_none()
                    {
                        ui.label(RichText::new("the node needs a Mesh component").color(egui::Color32::GRAY));
                    }
                },
            }

            changed = vector_edit(ui, "direction", "node space", &mut data.direction) || changed;
            changed = ui.add(egui::Slider::new(&mut data.cone_angle, 0.0..=180.0).text("cone angle (deg)")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.speed, 0.0..=100.0).text("speed")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.speed_var, 0.0..=100.0).text("speed var")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.life, 0.01..=60.0).logarithmic(true).text("life (s)")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.life_var, 0.0..=60.0).text("life var (s)")).changed() || changed;
        });

        ui.collapsing("Motion", |ui|
        {
            changed = ui.add(egui::Slider::new(&mut data.gravity_factor, -2.0..=2.0).text("gravity factor")).on_hover_text("negative: rises like fire").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.drag, 0.0..=10.0).text("drag")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.spin, -720.0..=720.0).text("spin (deg/s)")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.spin_var, 0.0..=720.0).text("spin var (deg/s)")).changed() || changed;
            changed = ui.checkbox(&mut data.random_rotation, "random start rotation").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.inherit_velocity, 0.0..=1.0).text("inherit velocity")).on_hover_text("share of the node velocity new particles take over (world space)").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.vortex, -100.0..=100.0).text("vortex")).on_hover_text("m/s² around the y axis of the node (tornado, portal, black hole)").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.attraction, -50.0..=50.0).text("attraction")).on_hover_text("m/s² toward the y axis of the node - negative pushes away").changed() || changed;
            changed = ui.checkbox(&mut data.attract_to_center, "attract to the center").on_hover_text("toward the node origin instead of its y axis").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.absorb_radius, 0.0..=10.0).text("absorb radius")).on_hover_text("m - particles closer to the axis (center) die, 0: off").changed() || changed;
            changed = ui.checkbox(&mut data.spread_spawns, "spread spawns over the way").on_hover_text("a moving node: the new particles of a frame lie along its way instead of in a clump").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.noise_strength, 0.0..=50.0).text("turbulence")).on_hover_text("m/s² - 0: off").changed() || changed;
            ui.add_enabled_ui(data.noise_strength > 0.0, |ui|
            {
                changed = ui.add(egui::Slider::new(&mut data.noise_frequency, 0.01..=10.0).logarithmic(true).text("turbulence frequency")).on_hover_text("1/m - higher: smaller swirls").changed() || changed;
                changed = ui.add(egui::Slider::new(&mut data.noise_speed, 0.0..=10.0).text("turbulence speed")).changed() || changed;
            });
        });

        ui.collapsing("Look", |ui|
        {
            changed = ui.add(egui::Slider::new(&mut data.size_start, 0.0..=10.0).text("size start")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.size_end, 0.0..=10.0).text("size end")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.size_var, 0.0..=10.0).text("size var")).changed() || changed;
            changed = enum_combo(ui, ui.make_persistent_id("particle_size_easing"), "size curve", "", &mut data.size_easing) || changed;

            changed = color_edit(ui, "color start", "", &mut data.color_start) || changed;
            changed = color_edit(ui, "color end", "", &mut data.color_end) || changed;
            changed = enum_combo(ui, ui.make_persistent_id("particle_color_easing"), "color curve", "", &mut data.color_easing) || changed;
            changed = ui.add(egui::Slider::new(&mut data.random_hue, 0.0..=1.0).text("random hue")).on_hover_text("each particle gets its color turned by up to this share of the color wheel (confetti)").changed() || changed;
            changed = enum_combo(ui, ui.make_persistent_id("particle_alpha_easing"), "alpha curve", "", &mut data.alpha_easing) || changed;
            changed = ui.add(egui::Slider::new(&mut data.fade_in, 0.0..=1.0).text("fade in")).on_hover_text("share of the life the alpha rises from 0").changed() || changed;

            ui.horizontal(|ui|
            {
                ui.label("blend:");
                changed = ui.selectable_value(&mut data.blend, ParticleBlend::Alpha, "Alpha").changed() || changed;
                changed = ui.selectable_value(&mut data.blend, ParticleBlend::Additive, "Additive").changed() || changed;
            });

            ui.add_enabled_ui(data.blend == ParticleBlend::Alpha, |ui|
            {
                changed = ui.checkbox(&mut data.sort, "sort by distance").on_hover_text("back to front per camera - only needed for alpha blend").changed() || changed;
            });

            changed = combo(ui, ui.make_persistent_id("particle_align"), "align", "Billboard: to the camera, Velocity: stretched along the way, Node: flat in the x/z plane of the node", &mut data.align, &[ParticleAlign::Billboard, ParticleAlign::Velocity, ParticleAlign::Node]) || changed;
            if data.align == ParticleAlign::Velocity
            {
                changed = ui.add(egui::Slider::new(&mut data.stretch, 0.0..=1.0).text("stretch (s)")).on_hover_text("the way of this time is drawn behind the particle").changed() || changed;
            }

            changed = ui.checkbox(&mut data.fog, "fog").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.softness, 0.0..=5.0).text("softness (m)")).on_hover_text("fades out close to the scene behind it - no hard edges at the ground, 0: off (not on WebGL)").changed() || changed;

            changed = ui.checkbox(&mut data.trail, "trail").on_hover_text("a ribbon behind every particle").changed() || changed;
            if data.trail
            {
                changed = ui.add(egui::Slider::new(&mut data.trail_length, 0.01..=5.0).logarithmic(true).text("trail length (s)")).changed() || changed;
                changed = ui.add(egui::Slider::new(&mut data.trail_width, 0.0..=2.0).text("trail width")).on_hover_text("share of the particle size").changed() || changed;
            }
            changed = ui.checkbox(&mut data.cast_shadows, "cast shadows").on_hover_text("hard shadow of the quads (alpha above about a third) - costs a draw per shadow view").changed() || changed;
            ui.add_enabled_ui(data.cast_shadows, |ui|
            {
                changed = ui.checkbox(&mut data.translucent_shadows, "translucent shadows").on_hover_text("the alpha lets light through (smoke) - needs \"Translucent Shadows\" in the rendering settings, else hard").changed() || changed;
            });

            ui.horizontal(|ui|
            {
                ui.label("texture:");
                let current = data.texture.as_ref().map(|texture| texture.read().unwrap().id);
                let selected = data.texture.as_ref().map(|texture| texture.read().unwrap().name.clone()).unwrap_or_else(|| "none (soft dot)".to_string());
                egui::ComboBox::from_id_salt(ui.make_persistent_id("particle_texture")).selected_text(selected).show_ui(ui, |ui|
                {
                    for texture in &self.runtime.texture_choices
                    {
                        let (id, name, path) = { let texture = texture.read().unwrap(); (texture.id, texture.name.clone(), texture.source.as_ref().map(|source| source.origin_path.clone()).unwrap_or_default()) };
                        if ui.selectable_label(current == Some(id), name).on_hover_text(path).clicked()
                        {
                            data.texture = OptionOrId::Some(texture.clone());
                            changed = true;
                        }
                    }
                }).response.on_hover_text("the loaded textures with a file - more via the texture menu");

                if data.texture.is_some() && ui.button("✖").on_hover_text("no texture (soft dot)").clicked()
                {
                    data.texture = OptionOrId::None;
                    changed = true;
                }
            });

            ui.horizontal(|ui|
            {
                ui.label("flipbook:");
                changed = ui.add(egui::DragValue::new(&mut data.flipbook[0]).range(1..=64).prefix("cols: ")).changed() || changed;
                changed = ui.add(egui::DragValue::new(&mut data.flipbook[1]).range(1..=64).prefix("rows: ")).changed() || changed;
            });
            changed = ui.add(egui::Slider::new(&mut data.flipbook_fps, 0.0..=60.0).text("flipbook fps")).on_hover_text("0: plays once over the life").changed() || changed;
        });

        ui.collapsing("Collision", |ui|
        {
            changed = ui.checkbox(&mut data.collide, "collide").changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.bounce, 0.0..=1.0).text("bounce")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.friction, 0.0..=1.0).text("friction")).changed() || changed;
            changed = ui.checkbox(&mut data.die_on_hit, "die on hit").on_hover_text("a hit ends the particle (rain drops) - reports a hit and a death for the sub emitters").changed() || changed;
            ui.add_enabled_ui(data.die_on_hit, |ui|
            {
                changed = ui.add(egui::Slider::new(&mut data.die_min_up, -1.0..=1.0).text("die min up")).on_hover_text("dies only on surfaces facing up at least this much (normal y) - steeper ones bounce (leaves on tree trunks), -1: every hit").changed() || changed;
            });
        });

        ui.collapsing("Events", |ui|
        {
            // the other emitters of the node - the editor leaves this one out of the list while drawing it
            let mut names: Vec<String> = node.as_ref().map(|node| node.read().unwrap().find_components::<ParticleEmitter>().iter().map(|component| component.read().unwrap().get_base().name.clone()).collect()).unwrap_or_default();
            names.retain(|name| *name != self.base.name);

            let sub_emitter = |ui: &mut egui::Ui, label: &str, id: &str, value: &mut String| -> bool
            {
                let mut picked = false;
                ui.horizontal(|ui|
                {
                    ui.label(label);
                    let shown = if value.is_empty() { "none".to_string() } else { value.clone() };
                    egui::ComboBox::from_id_salt(ui.make_persistent_id(id)).selected_text(shown).show_ui(ui, |ui|
                    {
                        if ui.selectable_label(value.is_empty(), "none").clicked() { value.clear(); picked = true; }
                        for name in &names
                        {
                            if ui.selectable_label(value == name, name).clicked() { *value = name.clone(); picked = true; }
                        }
                    });
                });
                picked
            };

            changed = sub_emitter(ui, "on death:", "particle_sub_death", &mut data.sub_emitter_death) || changed;
            changed = sub_emitter(ui, "on hit:", "particle_sub_hit", &mut data.sub_emitter_hit) || changed;

            if names.is_empty()
            {
                ui.label(RichText::new("sub emitters are other emitters of this node (rate 0, auto play off)").color(egui::Color32::GRAY));
            }

            changed = ui.add(egui::Slider::new(&mut data.sub_emitter_count, 1..=100).text("particles per event")).changed() || changed;
            changed = ui.add(egui::Slider::new(&mut data.sub_emitter_inherit, 0.0..=1.0).text("inherit velocity")).changed() || changed;
            changed = ui.checkbox(&mut data.report_events, "report events").on_hover_text("deaths and hits for code: events()").changed() || changed;
        });

        ui.collapsing("Sound", |ui|
        {
            // the Sound components of the node
            let names: Vec<String> = node.as_ref().map(|node| node.read().unwrap().find_components::<Sound>().iter().map(|component| component.read().unwrap().get_base().name.clone()).collect()).unwrap_or_default();

            let pick = |ui: &mut egui::Ui, label: &str, id: &str, info: &str, value: &mut String| -> bool
            {
                let mut picked = false;
                ui.horizontal(|ui|
                {
                    ui.label(label).on_hover_text(info);
                    let shown = if value.is_empty() { "none".to_string() } else { value.clone() };
                    egui::ComboBox::from_id_salt(ui.make_persistent_id(id)).selected_text(shown).show_ui(ui, |ui|
                    {
                        if ui.selectable_label(value.is_empty(), "none").clicked() { value.clear(); picked = true; }
                        for name in &names
                        {
                            if ui.selectable_label(value == name, name).clicked() { *value = name.clone(); picked = true; }
                        }
                    });
                });
                picked
            };

            changed = pick(ui, "loop:", "particle_sound_loop", "a Sound component of the node - plays while the emitter emits", &mut data.sound_loop) || changed;
            changed = pick(ui, "on burst:", "particle_sound_burst", "a Sound component of the node - played once at every burst and sub emitter event", &mut data.sound_on_burst) || changed;
            changed = ui.add(egui::Slider::new(&mut data.sound_max_per_second, 1.0..=60.0).text("max per second")).on_hover_text("one shots - more are dropped (rain)").changed() || changed;

            if names.is_empty()
            {
                ui.label(RichText::new("add a Sound component to the node (it is not started by itself)").color(egui::Color32::GRAY));
            }
        });

        ui.collapsing("Culling", |ui|
        {
            changed = ui.add(egui::Slider::new(&mut data.max_distance, 0.0..=1000.0).logarithmic(true).text("max distance")).on_hover_text("m to the camera - 0: no limit").changed() || changed;
            // keep running / pause / skip - two flags, older saves only know the pause
            let mut mode = if data.skip_when_hidden { 2 } else if data.pause_when_hidden { 1 } else { 0 };
            ui.horizontal(|ui|
            {
                ui.label("when hidden:").on_hover_text("no camera sees it - keep running: as always, pause: everything waits until it is seen again, skip: the time goes on but no new particles (an explosion off screen is over when looked at)");
                for (value, name) in [(0, "keep running"), (1, "pause"), (2, "skip")]
                {
                    if ui.selectable_value(&mut mode, value, name).changed()
                    {
                        data.pause_when_hidden = mode == 1;
                        data.skip_when_hidden = mode == 2;
                        changed = true;
                    }
                }
            });
            changed = ui.checkbox(&mut data.remove_when_done, "remove node when done").on_hover_text("one shot effects: the node goes away once all of its emitters are done").changed() || changed;
        });

        if changed
        {
            self.get_data_mut().set(data);
        }
    }
}
