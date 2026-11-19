use rustl::prelude::*;

rustl::app!(Game::default());

// the logic of the project - it runs while the game runs (Play in the editor, always in exports)
#[derive(Default)]
pub struct Game
{
    frames: u64,
}

impl App for Game
{
    fn init(&mut self, _context: &mut Context)
    {
        console_log!("game started");
    }

    // the scene is completely there - find nodes, cameras, ... here
    fn scene_loaded(&mut self, context: &mut Context, scene_id: u32)
    {
        let state = context.state.borrow();
        if let Some(scene) = state.scenes.iter().find(|scene| scene.id == scene_id)
        {
            console_log!("scene '{}' loaded: {} objects", scene.name, scene.nodes.len());
        }
    }

    // every frame before the scene update - input, game logic, moving nodes
    fn update(&mut self, _context: &mut Context)
    {
        self.frames += 1;
    }

    // every frame after physics, controllers and animations - things that follow a moving node (a lamp on a car, a marker on a bone)
    fn post_update(&mut self, _context: &mut Context)
    {
    }
}
