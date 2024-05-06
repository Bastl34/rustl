use std::collections::HashMap;

use super::{keyboard::Keyboard, mouse::Mouse, gamepad::Gamepad};

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum InputType
{
    Mouse,
    Keyboard,
    Gamepad,
    Unkown
}

pub struct InputManager
{
    pub keyboard: Keyboard,
    pub mouse: Mouse,
    pub gamepads: HashMap<usize, Gamepad>,

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

        for (_, gamepad) in &self.gamepads
        {
            if gamepad.has_input()
            {
                self.last_input_device = InputType::Gamepad;
            }
        }


        self.keyboard.update_states();
        self.mouse.update_states();

        for (_, gamepad) in &mut self.gamepads
        {
            gamepad.update_states();
        }
    }

    pub fn reset(&mut self)
    {
        self.keyboard.reset();
        self.mouse.reset();

        for (_, gamepad) in &mut self.gamepads
        {
            gamepad.reset();
        }
    }
}