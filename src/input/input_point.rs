use nalgebra::Vector2;
use strum_macros::EnumIter;

#[derive(EnumIter, Debug, PartialEq)]
pub enum PointState
{
    Down,
    Move,
    Up,
    Stationary
}

pub struct InputPoint
{
    pub id: u64,

    pub start_pos: Option<Vector2::<f32>>,
    pub last_pos: Option<Vector2::<f32>>,
    pub pos: Option<Vector2::<f32>>,
    pub velocity: Vector2::<f32>,

    pub state: PointState,

    pub first_action: u64,
    pub last_action: u64,

    pub first_action_frame: u64,
    pub last_action_frame: u64,
}

impl InputPoint
{
    pub fn new(id: u64) -> InputPoint
    {
        InputPoint
        {
            id: id,

            start_pos: None,
            last_pos: None,
            pos: None,
            velocity: Vector2::<f32>::zeros(),

            state: PointState::Stationary,

            first_action: 0,
            last_action: 0,

            first_action_frame: 0,
            last_action_frame: 0,
        }
    }
}