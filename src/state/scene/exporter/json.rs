use crate::state::scene::scene::Scene;
use crate::state::state::State;
use crate::helper::file::write_string_to_tile;

pub fn export(state: &State, path: &str) -> bool
{
    println!("exporting {} to json", path);

    let mut file_content = String::new();

    for scene in &state.scenes
    {
        //if !scene.export_json(path)
        //{
        //}
    }

    if write_string_to_tile(format!("{}.json", path).as_str(), file_content).is_ok()
    {
        return true;
    }

    false
}