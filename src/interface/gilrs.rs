use gilrs::Gilrs;

use crate::{helper::generic::get_secs, input::gamepad::{Gamepad, GamepadAxis, GamepadButton, GamepadPowerInfo}, state::state::State};

pub fn gilrs_initialize(state: &mut State, gilrs: &mut Gilrs)
{
    // gilrs lists only the connected ones - the rest keeps its player id until it times out
    let connected_uids: Vec<usize> = gilrs.gamepads().map(|(uid, _)| uid.into()).collect();
    for gamepad in state.io.input_manager.gamepads.values_mut()
    {
        if gamepad.connected && !connected_uids.contains(&gamepad.uid)
        {
            crate::console_log!("gamepad disconnected: {} (id {})", gamepad.name, gamepad.id);
            gamepad.connected = false;
            gamepad.last_update = get_secs();
        }
    }

    for (uid, gamepad) in gilrs.gamepads()
    {
        let uid: usize = uid.into();

        if state.io.input_manager.gamepad_by_uid_mut(uid).is_none()
        {
            let id = state.io.input_manager.free_gamepad_id();
            state.io.input_manager.gamepads.insert(id, Gamepad::new(uid, id, gamepad.name().to_string()));
            crate::console_log!("gamepad connected: {} (id {}, force feedback {})", gamepad.name(), id, gamepad.is_ff_supported());
        }

        let gamepad_input = state.io.input_manager.gamepad_by_uid_mut(uid).unwrap();
        if !gamepad_input.connected && gamepad.is_connected() { crate::console_log!("gamepad connected again: {} (id {})", gamepad.name(), gamepad_input.id); }

        gamepad_input.connected = gamepad.is_connected();
        gamepad_input.has_force_feedback = gamepad.is_ff_supported();
        gamepad_input.power_info = gilrs_map_power(gamepad.power_info());
    }

    // delete old disconnected gamepads
    state.io.input_manager.gamepads.retain(|_, gamepad|
    {
        !gamepad.can_be_deleted()
    });
}

// plays the rumbles asked for this frame - an effect stops when it is dropped, so it is kept until its time is up
#[cfg(not(target_arch = "wasm32"))]
pub fn gilrs_rumble(state: &mut State, gilrs: &mut Gilrs, playing: &mut Vec<(gilrs::ff::Effect, std::time::Instant)>)
{
    use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Repeat, Ticks};

    let now = std::time::Instant::now();
    playing.retain(|(_, until)| *until > now);

    for gamepad in state.io.input_manager.gamepads.values_mut()
    {
        let Some(rumble) = gamepad.rumble.take() else { continue; };
        let Some(id) = gilrs.gamepads().map(|(id, _)| id).find(|id| usize::from(*id) == gamepad.uid) else { continue; };

        let ms = (rumble.seconds * 1000.0).round().max(1.0) as u32;
        let scheduling = Replay { play_for: Ticks::from_ms(ms), ..Default::default() };
        let magnitude = |v: f32| (v.clamp(0.0, 1.0) * u16::MAX as f32) as u16;

        let mut builder = EffectBuilder::new();
        builder.add_effect(BaseEffect { kind: BaseEffectType::Strong { magnitude: magnitude(rumble.strong) }, scheduling, envelope: Default::default() });
        builder.add_effect(BaseEffect { kind: BaseEffectType::Weak { magnitude: magnitude(rumble.weak) }, scheduling, envelope: Default::default() });

        let Ok(effect) = builder.gamepads(&[id]).repeat(Repeat::For(Ticks::from_ms(ms))).finish(gilrs) else { continue; };

        if effect.play().is_ok()
        {
            playing.push((effect, now + std::time::Duration::from_millis(ms as u64 + 100)));
        }
    }
}

pub fn gilrs_event(state: &mut State, gilrs: &mut Gilrs, engine_frame: u64)
{
    let mut re_init = false;

    while let Some(gilrs::Event { id: uid, event, time: _ , .. }) = gilrs.next_event()
    {
        let uid: usize = uid.into();

        match event
        {
            gilrs::EventType::Connected | gilrs::EventType::Disconnected =>
            {
                re_init = true;
                continue;
            },
            _ => {},
        }

        let gamepad = state.io.input_manager.gamepad_by_uid_mut(uid);

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