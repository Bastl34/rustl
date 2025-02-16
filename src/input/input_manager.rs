use std::collections::HashMap;

use super::{gamepad::Gamepad, keyboard::Keyboard, mouse::Mouse, touch::Touch};

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
    pub gamepads: HashMap<usize, Gamepad>,
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
            gamepads: HashMap::new(),
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