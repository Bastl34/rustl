use std::collections::BTreeMap;

use super::{gamepad::Gamepad, input_point::InputPoint, keyboard::Keyboard, mouse::{Mouse, MouseButton}, touch::Touch};

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum InputType
{
    Mouse,
    Keyboard,
    Gamepad,
    Touch,
    Unkown
}

pub struct InputManager
{
    pub keyboard: Keyboard,
    pub mouse: Mouse,
    pub gamepads: BTreeMap<usize, Gamepad>, // by player id 1.., which stays while the gamepad is known
    pub touch: Touch,

    pub last_input_device: InputType
}

impl InputManager
{
    pub fn new() -> Self
    {
        Self
        {
            keyboard: Keyboard::new(),
            mouse: Mouse::new(),
            gamepads: BTreeMap::new(),
            touch: Touch::new(),

            last_input_device: InputType::Unkown
        }
    }

    pub fn update(&mut self)
    {
        if self.keyboard.has_input()
        {
            self.last_input_device = InputType::Keyboard;
        }
        else if self.mouse.has_input()
        {
            self.last_input_device = InputType::Mouse;
        }
        else if self.touch.has_input()
        {
            self.last_input_device = InputType::Touch;
        }

        for (_, gamepad) in &self.gamepads
        {
            if gamepad.has_input()
            {
                self.last_input_device = InputType::Gamepad;
            }
        }


        self.keyboard.update_states();
        self.mouse.update_states();
        self.touch.update_states();

        for (_, gamepad) in &mut self.gamepads
        {
            gamepad.update_states();
        }
    }

    // the lowest player id no gamepad holds, starting at 1
    pub fn free_gamepad_id(&self) -> usize
    {
        (1..).find(|id| !self.gamepads.contains_key(id)).unwrap()
    }

    // the gamepad of a player id, connected or not
    pub fn gamepad(&self, id: usize) -> Option<&Gamepad>
    {
        self.gamepads.get(&id)
    }

    pub fn gamepad_mut(&mut self, id: usize) -> Option<&mut Gamepad>
    {
        self.gamepads.get_mut(&id)
    }

    // the gamepad gilrs knows by this uid
    pub fn gamepad_by_uid_mut(&mut self, uid: usize) -> Option<&mut Gamepad>
    {
        self.gamepads.values_mut().find(|gamepad| gamepad.uid == uid)
    }

    pub fn get_pointer_input(&self) -> InputPoint
    {
        if let Some(touch) = self.touch.get_first_touch()
        {
            return touch.clone();
        }

        return self.mouse.point.clone();
    }

    pub fn is_main_pointer_action_active(&self) -> bool
    {
        if let Some(_touch) = self.touch.get_first_touch()
        {
            return true;
        }

        return self.mouse.is_holding(MouseButton::Left);
    }

    pub fn reset(&mut self)
    {
        self.keyboard.reset();
        self.mouse.reset();
        self.touch.reset();

        for (_, gamepad) in &mut self.gamepads
        {
            gamepad.reset();
        }
    }
}