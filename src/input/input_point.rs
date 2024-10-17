use nalgebra::{Vector2, Point2, distance};
use strum_macros::EnumIter;

#[derive(EnumIter, Debug, PartialEq, Clone, Copy)]
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

    pub start_pos: Option<Point2::<f32>>,
    pub last_pos: Option<Point2::<f32>>,
    pub pos: Option<Point2::<f32>>,
    pub velocity: Vector2::<f32>,

    pub state: PointState,

    pub force: Option<f32>,

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

            force: None,

            first_action: 0,
            last_action: 0,

            first_action_frame: 0,
            last_action_frame: 0,
        }
    }

    pub fn moved_distance(&self) -> f32
    {
        if self.pos.is_none() || self.start_pos.is_none()
        {
            return 0.0;
        }

        let pos = self.pos.unwrap();
        let start_pos = self.start_pos.unwrap();

        distance(&pos, &start_pos)
    }
}