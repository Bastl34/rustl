use std::sync::{RwLock, Arc};
use std::any::Any;

use crate::state::scene::node::{NodeItem, Node};

pub type ComponentItem = Box<dyn Component + Send + Sync>;
pub type SharedComponentItem = Arc<RwLock<Box<dyn Component + Send + Sync>>>;
//pub type SharedComponentItem = Arc<RwLock<Box<dyn Any + Send + Sync>>>;

pub trait Component: Any
{
    fn is_enabled(&self) -> bool;

    fn component_name(&self) -> &'static str;

    fn update(&mut self, time_delta: f32);

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    //fn get_data_<T: Component, U>(&self) -> &U;
    //fn get_data_mut_<T: Component, U>(&mut self) -> &U;

    //fn bla(&self);
    //fn bla<T: Component, U>(&self) -> &U;
}

/*
impl<T: Component + 'static> Component for T
{
    fn as_any(&self) -> &dyn Any
    {
        self
    }
}
*/

/*
#[macro_export]
macro_rules! find_shared_component
{
    ($vec:expr, $type:ty) =>
    {
        {
            let mut res: Option<&SharedComponentItem> = None;
            let value = $vec.iter().find
            (
                |c|
                {
                    let component_item = c.read().unwrap();
                    component_item.is::<$type>()
                }
            );
            if !value.is_some()
            {
                return;
            }

            res = Some(value.unwrap());
            res
        }
    };
}

#[macro_export]
macro_rules! find_shared_component_mut
{
    ($vec:expr, $type:ty) =>
    {
        {
            let mut res: Option<&SharedComponentItem> = None;
            let value = $vec.iter_mut().find
            (
                |c|
                {
                    let component_item = c.read().unwrap();
                    component_item.is::<$type>()
                }
            );
            if !value.is_some()
            {
                return;
            }

            res = Some(value.unwrap());
            res
        }
    };
}

#[macro_export]
macro_rules! shared_component_downcast
{
    ($component:expr, $type:ty) =>
    {
        {
            $component.downcast_ref::<$type>().unwrap()
        }
    };
}

#[macro_export]
macro_rules! shared_component_downcast_mut
{
    ($component:expr, $type:ty) =>
    {
        {
            $component.downcast_mut::<$type>().unwrap()
        }
    };
}

 */

#[macro_export]
macro_rules! new_shared_component
{
    ($component:expr) =>
    {
        {
            Arc::new(RwLock::new(Box::new($component)))
        }
    };
}

/*
let component = find_shared_component!(vec, Test).unwrap().read().unwrap();
let bla = shared_component_downcast!(component, Test);

//mut
let mut component = find_shared_component_mut!(vec, Test).unwrap().write().unwrap();
let bla = shared_component_downcast_mut!(component, Test);
 */