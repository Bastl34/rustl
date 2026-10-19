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

    fn update(&mut self, _context: &mut Context)
    {
        self.frames += 1;
    }
}
