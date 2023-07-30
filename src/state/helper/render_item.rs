use std::{any::Any};

pub type RenderItemType = Box<dyn RenderItem + Send + Sync>;
pub type RenderItemOption = Option<Box<dyn RenderItem + Send + Sync>>;

pub trait RenderItem: Any
{
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

#[macro_export]
macro_rules! render_item_impl_default
{
    () =>
    {
        fn as_any(&self) -> &dyn std::any::Any
        {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any
        {
            self
        }
    };
}

pub fn get_render_item<T>(render_item: &RenderItemType) -> Box<&T> where T: 'static
{
    let any = render_item.as_any();
    Box::new(any.downcast_ref::<T>().unwrap())
}

pub fn get_render_item_mut<T>(render_item: &mut RenderItemType) -> Box<&mut T> where T: 'static
{
    let any = render_item.as_any_mut();
    Box::new(any.downcast_mut::<T>().unwrap())
}