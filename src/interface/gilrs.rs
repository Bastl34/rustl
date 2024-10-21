use gilrs::Gilrs;

use crate::{input::gamepad::{Gamepad, GamepadAxis, GamepadButton, GamepadPowerInfo}, state::state::State};

pub fn gilrs_initialize(state: &mut State, gilrs: &mut Gilrs)
{
    for (id, gamepad) in gilrs.gamepads()
    {
        let id: usize = id.into();
        let mut gamepad_input = state.input_manager.gamepads.get_mut(&id);

        if gamepad_input.is_none()
        {
            state.input_manager.gamepads.insert(id, Gamepad::new(id, gamepad.name().to_string()));
            gamepad_input = state.input_manager.gamepads.get_mut(&id);
        }

        let gamepad_input = gamepad_input.unwrap();

        gamepad_input.connected = gamepad.is_connected();
        gamepad_input.has_force_feedback = gamepad.is_ff_supported();
        gamepad_input.power_info = gilrs_map_power(gamepad.power_info());
    }

    // delete old disconnected gamepads
    state.input_manager.gamepads.retain(|_, gamepad|
    {
        !gamepad.can_be_deleted()
    });
}

pub fn gilrs_event(state: &mut State, gilrs: &mut Gilrs, engine_frame: u64)
{
    let mut re_init = false;

    while let Some(gilrs::Event { id, event, time: _ , .. }) = gilrs.next_event()
    {
        let id: usize = id.into();
        let gamepad = state.input_manager.gamepads.get_mut(&id);

        if gamepad.is_none()
        {
            continue;
        }

        let gamepad = gamepad.unwrap();

        match event
        {
            gilrs::EventType::ButtonPressed(button, _code) =>
            {
                gamepad.set_button(gilrs_map_button(button), true, engine_frame);
            },
            gilrs::EventType::ButtonRepeated(button, _code) =>
            {
                gamepad.set_button(gilrs_map_button(button), true, engine_frame);
            },
            gilrs::EventType::ButtonReleased(button, _code) =>
            {
                gamepad.set_button(gilrs_map_button(button), false, engine_frame);
            },
            gilrs::EventType::ButtonChanged(button, value, _code) =>
            {
                gamepad.set_button_float(gilrs_map_button(button), value, engine_frame);
            },
            gilrs::EventType::AxisChanged(axis, value, _code) =>
            {
                gamepad.set_axis(gilrs_map_axis(axis), value, engine_frame);
            },
            gilrs::EventType::Connected => re_init = true,
            gilrs::EventType::Disconnected => re_init = true,
            gilrs::EventType::Dropped => {},
            gilrs::EventType::ForceFeedbackEffectCompleted => {},
            _ => {},
        }
    }

    if re_init
    {
        gilrs_initialize(state, gilrs);
    }
}

pub fn gilrs_map_power(power_info: gilrs::PowerInfo) -> GamepadPowerInfo
{
    match power_info
    {
        gilrs::PowerInfo::Unknown => GamepadPowerInfo::Unknown,
        gilrs::PowerInfo::Wired => GamepadPowerInfo::Wired,
        gilrs::PowerInfo::Discharging(level) => GamepadPowerInfo::Discharging(level),
        gilrs::PowerInfo::Charging(level) => GamepadPowerInfo::Charging(level),
        gilrs::PowerInfo::Charged => GamepadPowerInfo::Charged,
    }
}

pub fn gilrs_map_button(button: gilrs::Button) -> GamepadButton
{
    match button
    {
        gilrs::Button::South => GamepadButton::South,
        gilrs::Button::East => GamepadButton::East,
        gilrs::Button::North => GamepadButton::North,
        gilrs::Button::West => GamepadButton::West,
        gilrs::Button::C => GamepadButton::C,
        gilrs::Button::Z => GamepadButton::Z,
        gilrs::Button::LeftTrigger => GamepadButton::LeftBumper,
        gilrs::Button::LeftTrigger2 => GamepadButton::LeftTrigger,
        gilrs::Button::RightTrigger => GamepadButton::RightBumper,
        gilrs::Button::RightTrigger2 => GamepadButton::RightTrigger,
        gilrs::Button::Select => GamepadButton::Select,
        gilrs::Button::Start => GamepadButton::Start,
        gilrs::Button::Mode => GamepadButton::Mode,
        gilrs::Button::LeftThumb => GamepadButton::LeftThumb,
        gilrs::Button::RightThumb => GamepadButton::RightThumb,
        gilrs::Button::DPadUp => GamepadButton::DPadUp,
        gilrs::Button::DPadDown => GamepadButton::DPadDown,
        gilrs::Button::DPadLeft => GamepadButton::DPadLeft,
        gilrs::Button::DPadRight => GamepadButton::DPadRight,
        gilrs::Button::Unknown => GamepadButton::Unkown,
    }
}

pub fn gilrs_map_axis(axis: gilrs::Axis) -> GamepadAxis
{
    match axis
    {
        gilrs::Axis::LeftStickX => GamepadAxis::LeftStickX,
        gilrs::Axis::LeftStickY => GamepadAxis::LeftStickY,
        gilrs::Axis::LeftZ => GamepadAxis::LeftTrigger,
        gilrs::Axis::RightStickX => GamepadAxis::RightStickX,
        gilrs::Axis::RightStickY => GamepadAxis::RightStickY,
        gilrs::Axis::RightZ => GamepadAxis::RightTrigger,
        gilrs::Axis::DPadX => GamepadAxis::DPadX,
        gilrs::Axis::DPadY => GamepadAxis::DPadY,
        gilrs::Axis::Unknown => GamepadAxis::Unkown,
    }
}