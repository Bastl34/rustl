use crate::helper::generic::get_millis;

const KEY_PRESS_DEFAULT_THRESHOLD: f32 = 0.999;
const KEY_PRESS_DEFAULT_TIME: u64 = 250;
const KEY_PRESS_DEFAULT_LONG_PRESS_TIME: u64 = 600;

#[derive(PartialEq)]
pub enum PressStateType
{
    NotPressed = 0,
    Pressed,
    LongPressed
}

pub fn is_pressed_by_state(state: PressStateType) -> bool
{
    state == PressStateType::Pressed || state == PressStateType::LongPressed
}

pub struct PressState
{
    pub reset_on_pressed: bool, //needed for keys without key up events

    pub min_diff_time: u64,
    pub long_press_time: u64,

    pub threshold: f32,
    pub last_holding_value: f32,

    //times
    first_action_time: u64,
    last_press_time: u64,
    holding_time: u64,

    holding_state: bool
}

impl PressState
{
    pub fn new() -> PressState
    {
        PressState
        {
            reset_on_pressed: false,

            min_diff_time: KEY_PRESS_DEFAULT_TIME,
            long_press_time: KEY_PRESS_DEFAULT_LONG_PRESS_TIME,

            threshold: KEY_PRESS_DEFAULT_THRESHOLD,
            last_holding_value: 0.0,

            first_action_time: 0,
            last_press_time: 0,
            holding_time: 0,

            holding_state: false
        }
    }

    pub fn reset(&mut self, reset_last_pressed_time: bool)
    {
        self.holding_state = false;

        self.first_action_time = 0;

        if reset_last_pressed_time
        {
            self.last_press_time = 0;
        }

        self.holding_time = 0;
    }

    pub fn update_state(&mut self)
    {
        // if not holding -> reset holding time (and with this the holding state)
        // this is used for waited press and longPress
        // -> otherwise longPress etc will be true if it is asked (and was not pressed for some time)
        if !self.holding_state
        {
            self.holding_time = 0;
        }
    }

    fn determine_holding_time(&mut self)
    {
        self.holding_time = get_millis() as u64 - self.first_action_time;
    }

    pub fn update(&mut self, status: bool)
    {
        // do not update if the state is the same
        if self.holding_state == status
        {
            return;
        }

        self.determine_holding_time();

        if status
        {
            self.holding_state = true;
            self.last_holding_value = 1.0;

            if self.first_action_time == 0
            {
                self.first_action_time = get_millis();
            }
        }
        else
        {
            self.holding_state = false;
            self.first_action_time = 0;
            self.last_press_time = 0;
        }
    }

    pub fn update_float(&mut self, value: f32)
    {
        self.determine_holding_time();

        if value >= self.threshold || value <= -self.threshold
        {
            self.holding_state = true;
            self.last_holding_value = value;

            if self.first_action_time == 0
            {
                self.first_action_time = get_millis();
            }
        }
        else
        {
            self.holding_state = false;
            self.first_action_time = 0;
            self.last_press_time = 0;
        }
    }

    pub fn pressed(&mut self, wait_until_key_release: bool, ignore_long_press: bool) -> PressStateType
    {
        // wait until key is released -> for types like long press
        if wait_until_key_release && !self.reset_on_pressed && !self.holding_state && self.holding_time > 0
        {
            if self.holding_time > self.long_press_time
            {
                if !ignore_long_press
                {
                    // reset holding time
                    self.holding_time = 0;
                    self.last_press_time = get_millis();

                    return PressStateType::LongPressed;
                }
                else
                {
                    return PressStateType::NotPressed;
                }
            }
            else
            {
                // reset holding time
                self.holding_time = 0;
                self.last_press_time = get_millis();

                if self.reset_on_pressed
                {
                    self.update(false);
                }

                return PressStateType::Pressed;
            }
        }
        // do not wait until key is released -> for instant key press action
        else if (!wait_until_key_release || self.reset_on_pressed) && self.holding_state
        {
            if self.last_press_time + self.min_diff_time < get_millis()
            {
                self.holding_time = 0;
                self.last_press_time = get_millis();

                if self.reset_on_pressed
                {
                    self.update(false);
                }

                return PressStateType::Pressed;
            }
        }

        PressStateType::NotPressed
    }

    pub fn holding(&self) -> bool
    {
        if self.reset_on_pressed
        {
            false
        }
        else
        {
            self.holding_state
        }
    }

    pub fn holding_long(&self) -> bool
    {
        if self.reset_on_pressed
        {
            false
        }
        else
        {
            self.holding_time > self.long_press_time
        }
    }

}
