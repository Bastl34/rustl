use std::sync::OnceLock;

use super::context::Context;

// every method has an empty default - an app only implements what it needs
pub trait App
{
    fn init(&mut self, context: &mut Context)
    {
        let _ = context;
    }

    fn update(&mut self, context: &mut Context)
    {
        let _ = context;
    }

    fn resize(&mut self, context: &mut Context)
    {
        let _ = context;
    }

    fn exit(&mut self, context: &mut Context)
    {
        let _ = context;
    }

    // a scene is completely loaded (objects, cameras, lights) - also called after init for the scenes that were ready before
    fn scene_loaded(&mut self, context: &mut Context, scene_id: u32)
    {
        let _ = (context, scene_id);
    }

    fn allow_window_minimized_updates(&self) -> bool
    {
        false
    }

    fn request_exit(&mut self, context: &mut Context) -> bool
    {
        let _ = context;
        true
    }
}

// creates the app of a project - it lives while the game runs (Play)
pub type AppFactory = fn() -> Box<dyn App>;
pub const APP_FACTORY_SYMBOL: &str = "rustl_create_app";

static PROJECT_APP_FACTORY: OnceLock<AppFactory> = OnceLock::new();

// a build with the code of a project linked in (export) - see run_app
pub fn set_project_app_factory(factory: AppFactory)
{
    let _ = PROJECT_APP_FACTORY.set(factory);
}

pub fn project_app_factory() -> Option<AppFactory>
{
    PROJECT_APP_FACTORY.get().copied()
}
