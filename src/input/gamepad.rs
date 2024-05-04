#![allow(dead_code)]

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, Display, FromRepr};

use super::press_state::{PressState, is_pressed_by_state};

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum GamepadButton
{
    A,
    B,
    Y,
    X,
    C,
    Z,
    LeftTrigger,
    LeftBumper,
    RightTrigger,
    RightBumper,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,

    Unkown
}

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum GamepadAxis
{
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    DPadX,
    DPadY,
    LeftTrigger,
    RightTrigger,

    Unkown
}

pub struct Gamepad
{
    pub buttons: Vec<PressState>,
    pub axes: Vec<PressState>,
}

impl Gamepad
{
    pub fn new() -> Self
    {
        let button_vec = GamepadButton::iter().collect::<Vec<_>>();
        let axis_vec = GamepadAxis::iter().collect::<Vec<_>>();

        let button_states = button_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();
        let axis_states = axis_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();

        Self
        {
            buttons: button_states,
            axes: axis_states
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

    pub fn is_any_axis_active(&self) -> bool
    {
        for axis in &self.axes
        {
            if axis.holding()
            {
                return true;
            }
        }

        false
    }

    pub fn set_button(&mut self, button: GamepadButton, status: bool)
    {
        self.buttons[button as usize].update(status);
    }

    pub fn set_axis(&mut self, axis: GamepadAxis, value: f32)
    {
        self.axes[axis as usize].update_float(value);
    }

    pub fn update_states(&mut self)
    {
        for button in &mut self.buttons
        {
            button.update_state();
        }

        for axis in &mut self.axes
        {
            axis.update_state();
        }
    }

    pub fn reset(&mut self)
    {
        for button in &mut self.buttons
        {
            button.reset(true);
        }

        for axis in &mut self.axes
        {
            axis.reset(true);
        }
    }

    pub fn is_holding(&self, button: GamepadButton) -> bool
    {
        self.buttons[button as usize].holding()
    }

    pub fn is_holding_long(&self, button: GamepadButton) -> bool
    {
        self.buttons[button as usize].holding_long()
    }

    pub fn is_pressed(&mut self, button: GamepadButton) -> bool
    {
        let state = self.buttons[button as usize].pressed(true, false);
        is_pressed_by_state(state)
    }

    pub fn has_input(&self) -> bool
    {
        self.is_any_button_holding() || self.is_any_axis_active()
    }
}