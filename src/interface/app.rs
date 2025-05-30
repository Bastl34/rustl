use super::context::Context;

pub trait App
{
    fn init(&mut self, context: &mut Context);
    fn update(&mut self, context: &mut Context);
    fn resize(&mut self, context: &mut Context);
    fn exit(&mut self, context: &mut Context);

    fn request_exit(&mut self, context: &mut Context) -> bool
    {
        let _ = context;
        true
    }
}