use super::{keyboard::Keyboard, mouse::Mouse};

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum InputType
{
    Mouse,
    Keyboard,
    Unkown
}

pub struct InputManager
{
    pub keyboard: Keyboard,
    pub mouse: Mouse,

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


        self.keyboard.update_states();
        self.mouse.update_states();
    }

    pub fn reset(&mut self)
    {
        self.keyboard.reset();
        self.mouse.reset();
    }
}