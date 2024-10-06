#![allow(dead_code)]


use nalgebra::{Point2, Vector2};

use crate::helper::{generic, math};

use super::input_point::{InputPoint, PointState};

const TOUCH_MAX_TAP_MOVEMENT: f32 = 12.0;

pub struct Touch
{
    pub touches: Vec<InputPoint>,
}

impl Touch
{
    pub fn new() -> Self
    {
        Self
        {
            touches: vec![],
        }
    }

    pub fn has_touches(&self) -> bool
    {
        self.touches.len() > 0
    }

    fn get_touch_by_id_mut(&mut self, id: u64) -> Option<&mut InputPoint>
    {
        self.touches.iter_mut().find(|touch|
        {
            touch.id == id
        })
    }

    pub fn get_touch_by_id(&self, id: u64) -> Option<&InputPoint>
    {
        self.touches.iter().find(|touch|
        {
            touch.id == id
        })
    }

    pub fn get_first_touch(&self) -> Option<&InputPoint>
    {
        self.touches.get(0)
    }

    pub fn set(&mut self, id: u64, pos: Point2::<f32>, state: PointState, engine_frame: u64, force: Option<f32>)
    {
        let touch = self.get_touch_by_id_mut(id);

        // touch found
        if let Some(touch) = touch
        {
            // check if this point was updated inside the same frame
            if touch.last_action_frame == engine_frame
            {
                // do not set it to move when it was first set to down in this update cycle
                if touch.state != PointState::Down || state == PointState::Up
                {
                    touch.state = state;
                }
            }
            else
            {
                touch.state = state;
            }

            touch.force = force;

            // velocity update
            if state != PointState::Down && touch.pos.is_some()
            {
                touch.velocity += pos - touch.pos.unwrap();
            }
            else
            {
                touch.velocity = Vector2::<f32>::zeros();
            }

            // pos
            touch.pos = Some(pos);

            // action
            touch.last_action = generic::get_millis();
            touch.last_action_frame = engine_frame;
        }
        // new touch
        else
        {
            let mut new_touch = InputPoint::new(id);
            new_touch.first_action = generic::get_millis();
            new_touch.first_action_frame = engine_frame;

            new_touch.last_action = generic::get_millis();
            new_touch.last_action_frame = engine_frame;

            new_touch.start_pos = Some(pos);
            new_touch.pos = Some(pos);

            new_touch.force = force;
            new_touch.state = state;

            self.touches.push(new_touch);
        }
    }

    pub fn is_any_touch_holding(&self) -> bool
    {
        for touch in &self.touches
        {
            if touch.state == PointState::Stationary
            {
                return true;
            }
        }

        false
    }

    pub fn update_states(&mut self)
    {
        for touch in &mut self.touches
        {
            touch.last_pos = touch.pos.clone();
            touch.velocity = Vector2::<f32>::zeros();

            if touch.state != PointState::Up
            {
                touch.state = PointState::Stationary;
            }
        }

        self.touches.retain(|touch|
        {
            touch.state != PointState::Up
        });
    }

    pub fn reset(&mut self)
    {
        self.touches.clear();
    }

    pub fn has_input(&self) -> bool
    {
        self.has_velocity()
    }

    pub fn has_velocity(&self) -> bool
    {
        for touch in &self.touches
        {
            if !math::approx_zero_vec2(&touch.velocity)
            {
                return true;
            }
        }

        false
    }

    pub fn tapped_any(&mut self) -> Option<u64>
    {
        for touch in &self.touches
        {
            if touch.moved_distance() < TOUCH_MAX_TAP_MOVEMENT && touch.state == PointState::Up
            {
                return Some(touch.id);
            }
        }

        None
    }

    pub fn is_first_action(&self, current_engine_frame: u64) -> bool
    {
        for touch in &self.touches
        {
            if touch.first_action_frame == current_engine_frame && touch.state == PointState::Down
            {
                return true;
            }
        }

        false
    }
}