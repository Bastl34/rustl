#![allow(dead_code)]

use nalgebra::Vector2;
use strum::IntoEnumIterator;
use strum_macros::{EnumIter, Display, FromRepr};

use crate::{helper::{generic, math, change_tracker::ChangeTracker}, input::input_point::PointState};

use super::{press_state::{PressState, is_pressed_by_state}, input_point::InputPoint};

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum MouseButton
{
    Left,
    Right,
    Middle,
    Other1,
    Other2,
    Other3,
    Other4,
    Other5,
    Other6,
    Other7,
    Other8,

    Unkown
}

pub struct Mouse
{
    pub visible: ChangeTracker<bool>,
    pub buttons: Vec<PressState>,

    pub point: InputPoint,

    pub last_active_button: MouseButton,

    pub wheel_delta_x: f32,
    pub wheel_delta_y: f32,
}

impl Mouse
{
    pub fn new() -> Self
    {
        let button_vec = MouseButton::iter().collect::<Vec<_>>();

        let button_states = button_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();

        Self
        {
            visible: ChangeTracker::new(true),
            buttons: button_states,

            point: InputPoint::new(0),

            last_active_button: MouseButton::Unkown,

            wheel_delta_x: 0.0,
            wheel_delta_y: 0.0
        }
    }

    pub fn is_any_button_holding(&self) -> bool
    {
        for button in &self.buttons
        {
            if button.holding()
            {
                return true;
            }
        }

        false
    }

    pub fn set_button(&mut self, button: MouseButton, status: bool)
    {
        self.buttons[button as usize].update(status);

        if self.point.first_action == 0
        {
            self.point.first_action = generic::get_millis();
        }

        self.point.last_action = generic::get_millis();
        self.last_active_button = button;
    }

    pub fn set_pos(&mut self, pos: Vector2::<f32>, engine_frame: u64)
    {
        let pressed = self.is_any_button_holding();

        // todo: Check old eng
        //if self.visible
        //{
        if let Some(point_pos) = self.point.pos
        {
            self.point.velocity = pos - point_pos;
        }
        //}
        /*
        else
        {
            self.point.velocity = pos - Vector2::<f32>::new((width / 2.0).round(), height / 2.0).round())
        }
        */

        if self.point.start_pos.is_none()
        {
            self.point.start_pos = Some(pos);
        }

        self.point.pos = Some(pos);

        if self.point.first_action == 0
        {
            self.point.first_action = generic::get_millis();
            self.point.first_action_frame = engine_frame;
        }

        if pressed && (self.point.state == PointState::Up || self.point.state == PointState::Stationary)
        {
            self.point.state = PointState::Down;
            self.point.start_pos = Some(pos);

            self.point.first_action = generic::get_millis();
        }
        else if pressed && (self.point.state == PointState::Down || self.point.state == PointState::Move)
        {
          self.point.state = PointState::Move;
        }
        else if !pressed && (self.point.state == PointState::Move || self.point.state == PointState::Down)
        {
            self.point.state = PointState::Up;
        }
        else if self.point.state != PointState::Up || (self.point.state == PointState::Up && self.point.last_action_frame != engine_frame)
        {
            self.point.state = PointState::Stationary;
        }

		self.point.last_action = generic::get_millis();
		self.point.last_action_frame = engine_frame;
    }

    pub fn set_wheel_delta_x(&mut self, delta: f32)
    {
        self.wheel_delta_x = delta;
        self.point.last_action = generic::get_millis();
    }

    pub fn set_wheel_delta_y(&mut self, delta: f32)
    {
        self.wheel_delta_y = delta;
        self.point.last_action = generic::get_millis();
    }

    pub fn update_states(&mut self)
    {
        self.point.last_pos = self.point.pos;
        self.point.velocity = Vector2::<f32>::zeros();

        if self.point.state == PointState::Up
        {
            self.point.state = PointState::Stationary;
        }

        for button in &mut self.buttons
        {
            button.update_state();
        }

        self.wheel_delta_x = 0.0;
        self.wheel_delta_y = 0.0;
    }

    pub fn reset(&mut self)
    {
        for button in &mut self.buttons
        {
            button.reset(true);
        }

        self.point.state = PointState::Stationary;
    }

    pub fn is_holding(&self, button: MouseButton) -> bool
    {
        self.buttons[button as usize].holding()
    }

    pub fn is_holding_long(&self, button: MouseButton) -> bool
    {
        self.buttons[button as usize].holding_long()
    }

    pub fn is_pressed(&mut self, button: MouseButton) -> bool
    {
        let state = self.buttons[button as usize].pressed(false, false);
        is_pressed_by_state(state)
    }

    pub fn has_input(&self) -> bool
    {
        math::approx_zero_vec2(self.point.velocity) || self.is_any_button_holding()
    }
}