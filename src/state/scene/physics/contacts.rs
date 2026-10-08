#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use nalgebra::Vector3;
use rapier3d::prelude::*;

// A character capsule starts touching within this distance and lets go beyond the keep one - the gap in between stops flicker.
pub const CHARACTER_TOUCH_START: f32 = 0.03;
pub const CHARACTER_TOUCH_KEEP: f32 = 0.08;

// One side of a contact, as the scene knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContactTarget
{
    Object { node_id: u32, instance_id: Option<u32> }, // a physics object: its anchor node, plus the instance for a single mesh placement
    Vehicle { node_id: u32 },
    Character { node_id: u32 },
    GroundPlane,
}

// The variant of a ContactTarget without its data, so it can be compared with ==.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContactKind
{
    Object,
    Vehicle,
    Character,
    GroundPlane,
}

impl ContactTarget
{
    pub fn node_id(&self) -> Option<u32>
    {
        match self
        {
            ContactTarget::Object { node_id, .. } | ContactTarget::Vehicle { node_id } | ContactTarget::Character { node_id } => Some(*node_id),
            ContactTarget::GroundPlane => None,
        }
    }

    pub fn kind(&self) -> ContactKind
    {
        match self
        {
            ContactTarget::Object { .. } => ContactKind::Object,
            ContactTarget::Vehicle { .. } => ContactKind::Vehicle,
            ContactTarget::Character { .. } => ContactKind::Character,
            ContactTarget::GroundPlane => ContactKind::GroundPlane,
        }
    }
}

// Like touch events: Started once, Touching every frame in between, Stopped once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactPhase
{
    Started,
    Touching,
    Stopped,
}

#[derive(Clone, Copy, Debug)]
pub struct ContactEvent
{
    pub phase: ContactPhase,

    pub target: ContactTarget,
    pub other: ContactTarget,

    pub point: Vector3<f32>, // world space, the deepest point - the last one known once stopped
    pub normal: Vector3<f32>, // world space, from target towards other
    pub impact_speed: f32, // how fast both closed in when the contact started, kept for its whole duration
    pub relative_velocity: Vector3<f32>, // of other against target at the point
}

impl ContactEvent
{
    // the same contact seen from the other side
    pub fn flipped(&self) -> ContactEvent
    {
        ContactEvent
        {
            phase: self.phase,
            target: self.other,
            other: self.target,
            point: self.point,
            normal: -self.normal,
            impact_speed: self.impact_speed,
            relative_velocity: -self.relative_velocity,
        }
    }

    pub fn started(&self) -> bool
    {
        self.phase == ContactPhase::Started
    }

    pub fn touching(&self) -> bool
    {
        self.phase == ContactPhase::Touching
    }

    pub fn stopped(&self) -> bool
    {
        self.phase == ContactPhase::Stopped
    }
}

// Point, normal and relative velocity of a collider pair, from collider 1 towards collider 2.
#[derive(Clone, Copy)]
pub struct Measure
{
    pub point: Vector,
    pub normal: Vector,
    pub relative_velocity: Vector,
}

impl Measure
{
    fn flipped(self) -> Measure
    {
        Measure { point: self.point, normal: -self.normal, relative_velocity: -self.relative_velocity }
    }

    fn of(pair: &ContactPair, bodies: &RigidBodySet, colliders: &ColliderSet) -> Option<Measure>
    {
        let (manifold, contact) = pair.find_deepest_contact()?;
        let collider1 = colliders.get(pair.collider1)?;

        // the points of a trimesh contact are relative to the triangle
        let frame = match manifold.subshape_pos1()
        {
            Some(subshape) => *collider1.position() * *subshape,
            None => *collider1.position(),
        };

        let point = frame.transform_point(contact.local_p1);

        let velocity = |handle: ColliderHandle| colliders.get(handle)
            .and_then(|collider| collider.parent())
            .and_then(|body| bodies.get(body))
            .map(|body| body.velocity_at_point(point))
            .unwrap_or(Vector::ZERO);

        Some(Measure
        {
            point,
            normal: frame.rotation * manifold.local_n1,
            relative_velocity: velocity(pair.collider2) - velocity(pair.collider1),
        })
    }

    fn closing_speed(&self) -> f32
    {
        (-self.relative_velocity.dot(self.normal)).max(0.0)
    }
}

struct RawContact
{
    started: bool,
    colliders: (ColliderHandle, ColliderHandle),
    measure: Option<Measure>,
}

// Filled by rapier during the step, before the solver runs - the velocities are still the ones before the hit.
#[derive(Default)]
pub struct ContactCollector
{
    raw: Mutex<Vec<RawContact>>,
}

impl EventHandler for ContactCollector
{
    fn handle_collision_event(&self, bodies: &RigidBodySet, colliders: &ColliderSet, event: CollisionEvent, contact_pair: Option<&ContactPair>)
    {
        if event.sensor()
        {
            return;
        }

        let measure = contact_pair.and_then(|pair| Measure::of(pair, bodies, colliders));

        self.raw.lock().unwrap().push(RawContact { started: event.started(), colliders: (event.collider1(), event.collider2()), measure });
    }

    fn handle_contact_force_event(&self, _dt: Real, _bodies: &RigidBodySet, _colliders: &ColliderSet, _contact_pair: &ContactPair, _total_force_magnitude: Real)
    {
    }

    fn handle_soft_body_tear_event(&self, _soft_bodies: &SoftBodySet, _event: &SoftBodyTearEvent)
    {
    }
}

// target first, the smaller one by the derived order
type PairKey = (ContactTarget, ContactTarget);

// An object pair in contact, possibly through several collider pairs.
struct ActiveContact
{
    colliders: HashSet<(ColliderHandle, ColliderHandle)>, // target side first
    measure: Option<Measure>, // target towards other
    impact_speed: f32,
    fresh: bool, // started this frame, so it is not touching yet
}

// A collider within reach of a character capsule, measured from the capsule towards it.
pub struct CharacterTouch
{
    pub collider: ColliderHandle,
    pub distance: f32, // negative while the capsule overlaps it
    pub measure: Measure,
}

struct ActiveCharacterContact
{
    measure: Measure,
    impact_speed: f32,
}

// Merges rapier's per collider start and stop events into per object events, once per frame.
#[derive(Default)]
pub struct ContactTracker
{
    pub collector: ContactCollector,

    active: HashMap<PairKey, ActiveContact>,
    by_colliders: HashMap<(ColliderHandle, ColliderHandle), PairKey>, // see collider_key
    events: Vec<ContactEvent>,

    // characters are no rapier bodies - their controller hands over what the capsule touches, by character node id
    character_reports: HashMap<u32, Vec<CharacterTouch>>,
    character_active: HashMap<(u32, ContactTarget), ActiveCharacterContact>,
    characters_reported: HashSet<u32>, // this frame
}

impl ContactTracker
{
    pub fn events(&self) -> &[ContactEvent]
    {
        &self.events
    }

    pub fn active_amount(&self) -> usize
    {
        self.active.len() + self.character_active.len()
    }

    // anything process has to resolve colliders for
    pub fn has_pending(&self) -> bool
    {
        !self.character_reports.is_empty() || !self.collector.raw.lock().unwrap().is_empty()
    }

    // drops everything without reporting a stop - the colliders are gone or the run ended
    pub fn reset(&mut self)
    {
        self.collector.raw.lock().unwrap().clear();
        self.active.clear();
        self.by_colliders.clear();
        self.events.clear();

        self.character_reports.clear();
        self.character_active.clear();
        self.characters_reported.clear();
    }

    pub fn begin_frame(&mut self)
    {
        self.events.clear();
        self.characters_reported.clear();

        for active in self.active.values_mut()
        {
            active.fresh = false;
        }
    }

    // replaces what the character touched before, the next process turns the difference into events
    pub fn report_character(&mut self, node_id: u32, touches: Vec<CharacterTouch>)
    {
        self.character_reports.insert(node_id, touches);
    }

    pub fn remove_character(&mut self, node_id: u32)
    {
        self.character_reports.remove(&node_id);
        self.character_active.retain(|(character, _), _| *character != node_id);
    }

    // resolve: which object a collider belongs to, None for anything that does not report
    pub fn process(&mut self, resolve: impl Fn(ColliderHandle) -> Option<ContactTarget>)
    {
        let raw = std::mem::take(&mut *self.collector.raw.lock().unwrap());

        for contact in raw
        {
            let (handle1, handle2) = contact.colliders;

            if contact.started
            {
                self.start(handle1, handle2, contact.measure, &resolve);
            }
            else
            {
                self.stop(handle1, handle2);
            }
        }

        let reports = std::mem::take(&mut self.character_reports);

        for (node_id, touches) in reports
        {
            self.process_character(node_id, touches, &resolve);
        }
    }

    fn process_character(&mut self, node_id: u32, touches: Vec<CharacterTouch>, resolve: &impl Fn(ColliderHandle) -> Option<ContactTarget>)
    {
        let character = ContactTarget::Character { node_id };
        self.characters_reported.insert(node_id);

        // the closest part per object
        let mut closest: HashMap<ContactTarget, CharacterTouch> = HashMap::new();

        for touch in touches
        {
            let Some(other) = resolve(touch.collider) else { continue; };

            if closest.get(&other).is_none_or(|current| touch.distance < current.distance)
            {
                closest.insert(other, touch);
            }
        }

        let mut touching: HashSet<ContactTarget> = HashSet::new();

        for (other, touch) in closest
        {
            let key = (node_id, other);

            match self.character_active.get_mut(&key)
            {
                Some(active) if touch.distance <= CHARACTER_TOUCH_KEEP =>
                {
                    active.measure = touch.measure;
                    touching.insert(other);

                    self.events.push(Self::event(ContactPhase::Touching, (character, other), Some(active.measure), active.impact_speed));
                }
                None if touch.distance <= CHARACTER_TOUCH_START =>
                {
                    let impact_speed = touch.measure.closing_speed();
                    touching.insert(other);

                    self.character_active.insert(key, ActiveCharacterContact { measure: touch.measure, impact_speed });
                    self.events.push(Self::event(ContactPhase::Started, (character, other), Some(touch.measure), impact_speed));
                }
                _ => {}
            }
        }

        let ended: Vec<ContactTarget> = self.character_active.keys()
            .filter(|(character, other)| *character == node_id && !touching.contains(other))
            .map(|(_, other)| *other)
            .collect();

        for other in ended
        {
            if let Some(active) = self.character_active.remove(&(node_id, other))
            {
                self.events.push(Self::event(ContactPhase::Stopped, (character, other), Some(active.measure), active.impact_speed));
            }
        }
    }

    // Touching for everything still in contact, with the current point, and a stop for pairs whose colliders were removed.
    pub fn refresh(&mut self, bodies: &RigidBodySet, colliders: &ColliderSet, narrow_phase: &NarrowPhase)
    {
        let mut ended = vec![];

        for (key, active) in self.active.iter_mut()
        {
            let by_colliders = &mut self.by_colliders;

            active.colliders.retain(|&(a, b)|
            {
                let exists = colliders.get(a).is_some() && colliders.get(b).is_some();

                if !exists
                {
                    by_colliders.remove(&Self::collider_key(a, b));
                }

                exists
            });

            if active.colliders.is_empty()
            {
                ended.push(*key);
                continue;
            }

            if active.fresh
            {
                continue;
            }

            let current = active.colliders.iter().find_map(|&(a, b)|
            {
                let pair = narrow_phase.contact_pair(a, b)?;

                if !pair.has_any_active_contact()
                {
                    return None;
                }

                let measure = Measure::of(pair, bodies, colliders)?;

                Some(if pair.collider1 == a { measure } else { measure.flipped() })
            });

            if current.is_some()
            {
                active.measure = current;
            }

            self.events.push(Self::event(ContactPhase::Touching, *key, active.measure, active.impact_speed));
        }

        for key in ended
        {
            if let Some(active) = self.active.remove(&key)
            {
                self.events.push(Self::event(ContactPhase::Stopped, key, active.measure, active.impact_speed));
            }
        }

        // a character that did not look this frame, e.g. standing still with update only on movement, keeps what it touched
        for ((node_id, other), active) in &self.character_active
        {
            if !self.characters_reported.contains(node_id)
            {
                self.events.push(Self::event(ContactPhase::Touching, (ContactTarget::Character { node_id: *node_id }, *other), Some(active.measure), active.impact_speed));
            }
        }
    }

    fn start(&mut self, handle1: ColliderHandle, handle2: ColliderHandle, measure: Option<Measure>, resolve: &impl Fn(ColliderHandle) -> Option<ContactTarget>)
    {
        let (Some(target1), Some(target2)) = (resolve(handle1), resolve(handle2)) else { return; };

        if target1 == target2
        {
            return;
        }

        let (key, handles, measure) = if target1 <= target2
        {
            ((target1, target2), (handle1, handle2), measure)
        }
        else
        {
            ((target2, target1), (handle2, handle1), measure.map(Measure::flipped))
        };

        self.by_colliders.insert(Self::collider_key(handle1, handle2), key);

        match self.active.get_mut(&key)
        {
            Some(active) =>
            {
                active.colliders.insert(handles);
            }
            None =>
            {
                let impact_speed = measure.map(|measure| measure.closing_speed()).unwrap_or(0.0);

                self.active.insert(key, ActiveContact { colliders: HashSet::from([handles]), measure, impact_speed, fresh: true });
                self.events.push(Self::event(ContactPhase::Started, key, measure, impact_speed));
            }
        }
    }

    fn stop(&mut self, handle1: ColliderHandle, handle2: ColliderHandle)
    {
        let collider_key = Self::collider_key(handle1, handle2);

        let Some(key) = self.by_colliders.remove(&collider_key) else { return; };
        let Some(active) = self.active.get_mut(&key) else { return; };

        active.colliders.retain(|&(a, b)| Self::collider_key(a, b) != collider_key);

        if active.colliders.is_empty()
        {
            if let Some(active) = self.active.remove(&key)
            {
                self.events.push(Self::event(ContactPhase::Stopped, key, active.measure, active.impact_speed));
            }
        }
    }

    // the same for both orders of a pair
    fn collider_key(a: ColliderHandle, b: ColliderHandle) -> (ColliderHandle, ColliderHandle)
    {
        if a.into_raw_parts() <= b.into_raw_parts() { (a, b) } else { (b, a) }
    }

    fn event(phase: ContactPhase, key: PairKey, measure: Option<Measure>, impact_speed: f32) -> ContactEvent
    {
        let v = |v: Vector| Vector3::new(v.x, v.y, v.z);
        let measure = measure.unwrap_or(Measure { point: Vector::ZERO, normal: Vector::ZERO, relative_velocity: Vector::ZERO });

        ContactEvent
        {
            phase,
            target: key.0,
            other: key.1,
            point: v(measure.point),
            normal: v(measure.normal),
            impact_speed,
            relative_velocity: v(measure.relative_velocity),
        }
    }
}
