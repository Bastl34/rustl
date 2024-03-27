#![allow(dead_code)]

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, Display, FromRepr};

use super::press_state::{PressState, PressStateType, is_pressed_by_state};

#[derive(EnumIter, Debug, PartialEq, Clone, Copy, Display, FromRepr)]
pub enum Key
{
    Key1 = 0,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Key0,

    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    Escape,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    Snapshot,
    Scroll,
    Pause,

    Insert,
    Home,
    Delete,
    End,
    PageDown,
    PageUp,

    Left,
    Up,
    Right,
    Down,

    Backspace,
    Return,
    Space,

    Compose,

    Caret,

    Numlock,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadDivide,
    NumpadDecimal,
    NumpadComma,
    NumpadEnter,
    NumpadEquals,
    NumpadMultiply,
    NumpadSubtract,

    AbntC1,
    AbntC2,
    Apostrophe,
    Apps,
    Asterisk,
    At,
    Ax,
    Backslash,
    Calculator,
    Capital,
    Colon,
    Comma,
    Convert,
    Equals,
    Grave,
    Kana,
    Kanji,
    LAlt,
    LBracket,
    LControl,
    LShift,
    LWin,
    Mail,
    MediaSelect,
    MediaStop,
    Minus,
    Mute,
    MyComputer,
    NavigateForward,
    NavigateBackward,
    NextTrack,
    NoConvert,
    OEM102,
    Period,
    PlayPause,
    Plus,
    Power,
    PrevTrack,
    RAlt,
    RBracket,
    RControl,
    RShift,
    RWin,
    Semicolon,
    Slash,
    Sleep,
    Stop,
    Sysrq,
    Tab,
    Underline,
    Unlabeled,
    VolumeDown,
    VolumeUp,
    Wake,
    WebBack,
    WebFavorites,
    WebForward,
    WebHome,
    WebRefresh,
    WebSearch,
    WebStop,
    Yen,
    Copy,
    Paste,
    Cut,
}

pub fn get_keys_as_string_vec() -> Vec<String>
{
    let key_vec: Vec<Key> = Key::iter().collect::<Vec<_>>();
    key_vec.iter().map(|key| { key.to_string() }).collect::<Vec<_>>()
}

#[derive(EnumIter, Debug, PartialEq, Display)]
pub enum Modifier
{
    Shift = 0,
    Ctrl,
    Alt,
    Logo
}

pub struct Keyboard
{
    keys: Vec<PressState>,
    modifiers: Vec<PressState>,
}

impl Keyboard
{
    pub fn new() -> Self
    {
        let key_vec = Key::iter().collect::<Vec<_>>();
        let modifiers_vec = Modifier::iter().collect::<Vec<_>>();

        let key_states = key_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();
        let mod_states = modifiers_vec.iter().map(|_key| { PressState::new() }).collect::<Vec<_>>();

        Self
        {
            keys: key_states,
            modifiers: mod_states,
        }
    }

    pub fn set_key(&mut self, key: Key, status: bool)
    {
        self.keys[key as usize].update(status);
    }

    pub fn set_modifier(&mut self, modifier: Modifier, status: bool)
    {
        self.modifiers[modifier as usize].update(status);
    }

    pub fn update_states(&mut self)
    {
        for key in &mut self.keys
        {
            key.update_state();
        }

        for modifier in &mut self.modifiers
        {
            modifier.update_state();
        }
    }

    pub fn reset(&mut self)
    {
        for key in &mut self.keys
        {
            key.reset(true);
        }

        for modifier in &mut self.modifiers
        {
            modifier.reset(true);
        }
    }

    pub fn is_any_key_holding(&self) -> bool
    {
        for key in &self.keys
        {
            if key.holding()
            {
                return true;
            }
        }

        for modifier in &self.modifiers
        {
            if modifier.holding()
            {
                return true;
            }
        }

        false
    }

    pub fn is_holding(&self, key: Key) -> bool
    {
        self.keys[key as usize].holding()
    }

    pub fn is_holding_by_keys(&self, keys: &Vec<Key>) -> bool
    {
        for key in keys
        {
            if self.keys[*key as usize].holding()
            {
                return true;
            }
        }

        false
    }

    pub fn is_holding_modifier(&self, modifier: Modifier) -> bool
    {
        self.modifiers[modifier as usize].holding()
    }

    pub fn get_pressed_state(&mut self, key: Key, wait_until_key_release: bool, ignore_long_press: bool) -> PressStateType
    {
        self.keys[key as usize].pressed(wait_until_key_release, ignore_long_press)
    }

    pub fn get_pressed_state_modifier(&mut self, modifier: Modifier, wait_until_key_release: bool, ignore_long_press: bool) -> PressStateType
    {
        self.modifiers[modifier as usize].pressed(wait_until_key_release, ignore_long_press)
    }

    pub fn is_pressed(&mut self, key: Key) -> bool
    {
        let state = self.keys[key as usize].pressed(true, false);
        is_pressed_by_state(state)
    }

    pub fn is_pressed_no_wait(&mut self, key: Key) -> bool
    {
        let state = self.keys[key as usize].pressed(false, false);
        is_pressed_by_state(state)
    }

    pub fn is_pressed_by_keys(&mut self, keys: &Vec<Key>) -> bool
    {
        let mut result = false;
        for key in keys
        {
            if self.keys[*key as usize].holding()
            {
                let state = self.keys[*key as usize].pressed(true, false);
                result =  is_pressed_by_state(state) || result;
            }
        }

        result
    }

    pub fn is_pressed_modifier(&mut self, modifier: Modifier) -> bool
    {
        let state = self.modifiers[modifier as usize].pressed(true, false);
        is_pressed_by_state(state)
    }

    pub fn has_input(&self) -> bool
    {
        self.is_any_key_holding()
    }
}