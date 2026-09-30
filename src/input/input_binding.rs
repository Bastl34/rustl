#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, Display};

use crate::gui::helper::property_items::enum_combo;

use super::{gamepad::{Gamepad, GamepadAxis, GamepadButton}, input_manager::InputManager, keyboard::{Key, Modifier}, mouse::MouseButton, press_state::is_pressed_by_state};

// below this a stick or trigger counts as released
pub const INPUT_DEADZONE: f32 = 0.12;

#[derive(EnumIter, Display, Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum AxisDirection
{
    Positive,
    Negative,
}

// one key, button or stick direction
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum InputSource
{
    Key(Key),
    Modifier(Modifier),
    Mouse(MouseButton),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxis, AxisDirection),
}

// which gamepads an input reads - Player is the id from InputManager::gamepads
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub enum GamepadSelect
{
    None,
    #[default]
    Any,
    Player(usize),
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ActionValue
{
    pub value: f32, // 0..1
    pub analog: bool, // the value came from a stick or trigger
}

// an action is triggered by any of its sources
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(transparent)]
pub struct InputAction
{
    pub sources: Vec<InputSource>,
}

impl InputSource
{
    fn is_gamepad(&self) -> bool
    {
        matches!(self, InputSource::GamepadButton(_) | InputSource::GamepadAxis(..))
    }

    fn gamepad_value(&self, gamepad: &Gamepad) -> ActionValue
    {
        let (value, analog) = match *self
        {
            InputSource::GamepadButton(button) => (gamepad.buttons[button as usize].holding_value, false),
            InputSource::GamepadAxis(axis, direction) =>
            {
                let value = gamepad.axes[axis as usize];
                (if direction == AxisDirection::Positive { value } else { -value }, true)
            }
            _ => (0.0, false),
        };

        if value > INPUT_DEADZONE { ActionValue { value: value.min(1.0), analog } } else { ActionValue::default() }
    }

    pub fn label(&self) -> String
    {
        match self
        {
            InputSource::Key(key) => key.to_string(),
            InputSource::Modifier(modifier) => modifier.to_string(),
            InputSource::Mouse(button) => format!("Mouse {}", button),
            InputSource::GamepadButton(button) => format!("Pad {}", button),
            InputSource::GamepadAxis(axis, AxisDirection::Positive) => format!("Pad {} +", axis),
            InputSource::GamepadAxis(axis, AxisDirection::Negative) => format!("Pad {} -", axis),
        }
    }
}

fn selected_gamepads<'a>(io: &'a InputManager, select: GamepadSelect) -> impl Iterator<Item = &'a Gamepad>
{
    io.gamepads.values().filter(move |gamepad| gamepad.connected && match select
    {
        GamepadSelect::None => false,
        GamepadSelect::Any => true,
        GamepadSelect::Player(id) => gamepad.id == id,
    })
}

impl InputAction
{
    pub fn new(sources: &[InputSource]) -> Self
    {
        Self { sources: sources.to_vec() }
    }

    // the strongest of all sources
    pub fn value(&self, io: &InputManager, gamepad: GamepadSelect) -> ActionValue
    {
        let mut result = ActionValue::default();

        for source in &self.sources
        {
            let value = match *source
            {
                InputSource::Key(key) => ActionValue { value: io.keyboard.is_holding(key) as i32 as f32, analog: false },
                InputSource::Modifier(modifier) => ActionValue { value: io.keyboard.is_holding_modifier(modifier) as i32 as f32, analog: false },
                InputSource::Mouse(button) => ActionValue { value: io.mouse.is_holding(button) as i32 as f32, analog: false },
                _ => selected_gamepads(io, gamepad).map(|pad| source.gamepad_value(pad)).fold(ActionValue::default(), |a, b| if b.value > a.value { b } else { a }),
            };

            if value.value > result.value
            {
                result = value;
            }
        }

        result
    }

    pub fn held(&self, io: &InputManager, gamepad: GamepadSelect) -> bool
    {
        self.value(io, gamepad).value > 0.0
    }

    // once per press - every source is asked, so none keeps a pending press for later
    pub fn pressed(&self, io: &mut InputManager, gamepad: GamepadSelect) -> bool
    {
        let mut result = false;

        for source in &self.sources
        {
            let pressed = match *source
            {
                InputSource::Key(key) => io.keyboard.is_pressed(key),
                InputSource::Modifier(modifier) => io.keyboard.is_pressed_modifier(modifier),
                InputSource::Mouse(button) => io.mouse.is_pressed(button),
                _ =>
                {
                    let mut any = false;
                    for pad in io.gamepads.values_mut().filter(|pad| pad.connected && match gamepad
                    {
                        GamepadSelect::None => false,
                        GamepadSelect::Any => true,
                        GamepadSelect::Player(id) => pad.id == id,
                    })
                    {
                        any |= match *source
                        {
                            InputSource::GamepadButton(button) => pad.is_pressed(button),
                            InputSource::GamepadAxis(axis, direction) =>
                            {
                                let side = (pad.axes[axis as usize] > 0.0) == (direction == AxisDirection::Positive);
                                let state = pad.axes_press_states[axis as usize].pressed(true, false);
                                side && is_pressed_by_state(state)
                            }
                            _ => false,
                        };
                    }
                    any
                }
            };

            result |= pressed;
        }

        result
    }

    pub fn uses_gamepad(&self) -> bool
    {
        self.sources.iter().any(|source| source.is_gamepad())
    }
}

// ********** ui **********

#[derive(Clone, Copy, PartialEq)]
enum SourceKind
{
    Key,
    Modifier,
    Mouse,
    GamepadButton,
    GamepadAxis,
}

impl SourceKind
{
    const ALL: [SourceKind; 5] = [SourceKind::Key, SourceKind::Modifier, SourceKind::Mouse, SourceKind::GamepadButton, SourceKind::GamepadAxis];

    fn of(source: &InputSource) -> Self
    {
        match source
        {
            InputSource::Key(_) => SourceKind::Key,
            InputSource::Modifier(_) => SourceKind::Modifier,
            InputSource::Mouse(_) => SourceKind::Mouse,
            InputSource::GamepadButton(_) => SourceKind::GamepadButton,
            InputSource::GamepadAxis(..) => SourceKind::GamepadAxis,
        }
    }

    fn name(&self) -> &'static str
    {
        match self
        {
            SourceKind::Key => "Key",
            SourceKind::Modifier => "Modifier",
            SourceKind::Mouse => "Mouse",
            SourceKind::GamepadButton => "Pad Button",
            SourceKind::GamepadAxis => "Pad Axis",
        }
    }

    fn default_source(&self) -> InputSource
    {
        match self
        {
            SourceKind::Key => InputSource::Key(Key::Space),
            SourceKind::Modifier => InputSource::Modifier(Modifier::LeftShift),
            SourceKind::Mouse => InputSource::Mouse(MouseButton::Left),
            SourceKind::GamepadButton => InputSource::GamepadButton(GamepadButton::South),
            SourceKind::GamepadAxis => InputSource::GamepadAxis(GamepadAxis::LeftStickX, AxisDirection::Positive),
        }
    }
}

// one row per source with kind and key, plus add and remove
pub fn input_action_ui(ui: &mut egui::Ui, id_salt: &str, label: &str, action: &mut InputAction)
{
    let id = egui::Id::new(id_salt).with(label);

    ui.label(label);
    ui.indent(id, |ui|
    {
        let mut remove = None;

        for (index, source) in action.sources.iter_mut().enumerate()
        {
            ui.horizontal(|ui|
            {
                let mut kind = SourceKind::of(source);
                egui::ComboBox::from_id_salt(id.with(index).with("kind")).selected_text(kind.name()).width(90.0).show_ui(ui, |ui|
                {
                    for option in SourceKind::ALL
                    {
                        ui.selectable_value(&mut kind, option, option.name());
                    }
                });

                if kind != SourceKind::of(source)
                {
                    *source = kind.default_source();
                }

                let value_id = id.with(index).with("value");
                match source
                {
                    InputSource::Key(key) => { enum_combo(ui, value_id, "", "", key); }
                    InputSource::Modifier(modifier) => { enum_combo(ui, value_id, "", "", modifier); }
                    InputSource::Mouse(button) => { enum_combo(ui, value_id, "", "", button); }
                    InputSource::GamepadButton(button) => { enum_combo(ui, value_id, "", "", button); }
                    InputSource::GamepadAxis(axis, direction) =>
                    {
                        enum_combo(ui, value_id, "", "", axis);
                        enum_combo(ui, value_id.with("direction"), "", "", direction);
                    }
                }

                if ui.small_button("×").on_hover_text("remove").clicked()
                {
                    remove = Some(index);
                }
            });
        }

        if let Some(index) = remove
        {
            action.sources.remove(index);
        }

        if ui.small_button("+").on_hover_text("add a key or button").clicked()
        {
            action.sources.push(InputSource::Key(Key::Space));
        }
    });
}

pub fn gamepad_select_ui(ui: &mut egui::Ui, id_salt: &str, select: &mut GamepadSelect)
{
    ui.horizontal(|ui|
    {
        ui.label("Gamepad");

        let text = match select
        {
            GamepadSelect::None => "None".to_string(),
            GamepadSelect::Any => "Any".to_string(),
            GamepadSelect::Player(id) => format!("Player {}", id),
        };

        egui::ComboBox::from_id_salt(egui::Id::new(id_salt).with("gamepad")).selected_text(text).show_ui(ui, |ui|
        {
            ui.selectable_value(select, GamepadSelect::None, "None");
            ui.selectable_value(select, GamepadSelect::Any, "Any");
            for id in 1..=8
            {
                ui.selectable_value(select, GamepadSelect::Player(id), format!("Player {}", id));
            }
        }).response.on_hover_text("Any reads every gamepad, Player n only the gamepad with that id (they count from 1 in the order they connect)");
    });
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn an_action_is_saved_by_names()
    {
        let action = InputAction::new(&[InputSource::Key(Key::W), InputSource::GamepadAxis(GamepadAxis::LeftStickX, AxisDirection::Negative)]);
        let json = serde_json::to_string(&action).unwrap();

        assert_eq!(json, r#"[{"Key":"W"},{"GamepadAxis":["LeftStickX","Negative"]}]"#);
        assert_eq!(serde_json::from_str::<InputAction>(&json).unwrap(), action);
    }

    #[test]
    fn any_of_the_keys_triggers_the_action()
    {
        let mut io = InputManager::new();
        let action = InputAction::new(&[InputSource::Key(Key::W), InputSource::Key(Key::ArrowUp)]);

        assert!(!action.held(&io, GamepadSelect::Any));
        io.keyboard.set_key(Key::ArrowUp, true, 1);
        assert!(action.held(&io, GamepadSelect::Any));
        assert_eq!(action.value(&io, GamepadSelect::Any), ActionValue { value: 1.0, analog: false });
    }

    // the events in the order gilrs sends them for an analog trigger: pressed before its value, released before its value
    #[test]
    fn a_trigger_follows_its_value_and_lets_go()
    {
        let mut io = InputManager::new();
        io.gamepads.insert(1, Gamepad::new(0, 1, "pad".to_string()));
        let throttle = InputAction::new(&[InputSource::GamepadButton(GamepadButton::RightTrigger)]);
        fn pad(io: &mut InputManager) -> &mut Gamepad { io.gamepads.get_mut(&1).unwrap() }

        pad(&mut io).set_button_float(GamepadButton::RightTrigger, 0.4, 1);
        assert_eq!(throttle.value(&io, GamepadSelect::Any).value, 0.4, "half pressed");

        pad(&mut io).set_button(GamepadButton::RightTrigger, true, 2);
        pad(&mut io).set_button_float(GamepadButton::RightTrigger, 0.8, 2);
        assert_eq!(throttle.value(&io, GamepadSelect::Any).value, 0.8);

        pad(&mut io).set_button(GamepadButton::RightTrigger, false, 3);
        pad(&mut io).set_button_float(GamepadButton::RightTrigger, 0.5, 3);
        pad(&mut io).set_button_float(GamepadButton::RightTrigger, 0.0, 4);
        assert_eq!(throttle.value(&io, GamepadSelect::Any).value, 0.0, "released, nothing may stick");
    }

    #[test]
    fn only_the_selected_gamepad_counts()
    {
        let mut io = InputManager::new();
        for id in 1..=2
        {
            io.gamepads.insert(id, Gamepad::new(id + 10, id, format!("pad {}", id)));
        }
        io.gamepads.get_mut(&2).unwrap().set_axis(GamepadAxis::LeftStickX, -0.6, 1);

        let left = InputAction::new(&[InputSource::GamepadAxis(GamepadAxis::LeftStickX, AxisDirection::Negative)]);

        assert_eq!(left.value(&io, GamepadSelect::Player(1)).value, 0.0);
        assert_eq!(left.value(&io, GamepadSelect::Player(2)), ActionValue { value: 0.6, analog: true });
        assert_eq!(left.value(&io, GamepadSelect::Any).value, 0.6);
        assert_eq!(left.value(&io, GamepadSelect::None).value, 0.0);
    }
}
