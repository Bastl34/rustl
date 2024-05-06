#![allow(dead_code)]

use nalgebra::ComplexField;
use strum::IntoEnumIterator;
use strum_macros::{EnumIter, Display, FromRepr};

use crate::helper::generic::get_secs;

use super::press_state::{PressState, is_pressed_by_state};

const GAMEPAD_MAX_TIMEOUT: u64 = 5 * 60 * 1000;
const DEFAULT_DEADZONE: f32 = 0.1;

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum GamepadButton
{
    South, // xBox: A
    East, // xBox: B
    North, // xBox: Y
    West, // xBox: X
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GamepadPowerInfo
{
    Wired,
    Discharging(u8),
    Charging(u8),
    Charged,

    Unknown
}

pub struct Gamepad
{
    pub name: String,
    pub id: usize,

    pub connected: bool,
    pub has_force_feedback: bool,

    pub power_info: GamepadPowerInfo,

    pub buttons: Vec<PressState>,
    pub axes: Vec<f32>,
    pub axes_press_states: Vec<PressState>,

    pub axis_deadzone: f32,

    pub last_update: u64,
}

impl Gamepad
{
    pub fn new(id: usize, name: String) -> Self
    {
        let button_vec = GamepadButton::iter().collect::<Vec<_>>();
        let axis_vec = GamepadAxis::iter().collect::<Vec<_>>();

        let button_states = button_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();
        let axes_states = axis_vec.iter().map(|_key| { 0.0 }).collect::<Vec<_>>();
        let axes_press_states = axis_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();

        Self
        {
            name,
            id,

            connected: true,
            has_force_feedback: false,

            power_info: GamepadPowerInfo::Unknown,

            buttons: button_states,
            axes: axes_states,
            axes_press_states,

            axis_deadzone: DEFAULT_DEADZONE,

            last_update: get_secs()
        }
    }

    pub fn can_be_deleted(&self) -> bool
    {
        !self.connected && self.last_update + GAMEPAD_MAX_TIMEOUT < get_secs()
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
            if axis.abs() > self.axis_deadzone
            {
                return true;
            }
        }

        false
    }

    pub fn set_button(&mut self, button: GamepadButton, status: bool)
    {
        self.buttons[button as usize].update(status);

        self.last_update = get_secs();
    }

    pub fn set_button_float(&mut self, button: GamepadButton, value: f32)
    {
        self.buttons[button as usize].update_float(value);

        self.last_update = get_secs();
    }

    pub fn set_axis(&mut self, axis: GamepadAxis, value: f32)
    {
        self.axes[axis as usize] = value;
        self.axes_press_states[axis as usize].update_float(value);

        self.last_update = get_secs();
    }

    pub fn get_axis_value(&mut self, axis: GamepadAxis) -> f32
    {
        self.axes[axis as usize]
    }

    pub fn update_states(&mut self)
    {
        for button in &mut self.buttons
        {
            button.update_state();
        }

        for axis in &mut self.axes_press_states
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

        for axis in &mut self.axes_press_states
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

    pub fn is_axis_active(&self, axis: GamepadAxis) -> bool
    {
        self.axes[axis as usize].abs() > self.axis_deadzone
    }

    pub fn has_input(&self) -> bool
    {
        self.is_any_button_holding() || self.is_any_axis_active()
    }
}