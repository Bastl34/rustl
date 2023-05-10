pub type ComponentItem = Box<dyn Component + Send + Sync>;

pub trait Component
{
    fn is_enabled(&self) -> bool;

    fn update(&mut self, time_delta: f32);
}