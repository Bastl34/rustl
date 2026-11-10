use serde::{Deserialize, Serialize};

// origin of a node, instance, component, extra or tag - decides what the editor saves (see saved_state)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Origin
{
    // set by code (engine, editor, app) - not saved, not editable in the editor
    #[default]
    Runtime,
    // loaded from the asset - only changes are saved
    Asset,
    // read from the scene file or set in the editor - saved with the scene
    Scene,
}

impl Origin
{
    pub fn name(&self) -> &'static str
    {
        match self
        {
            Origin::Runtime => "runtime",
            Origin::Asset => "asset",
            Origin::Scene => "scene",
        }
    }

    pub fn description(&self) -> &'static str
    {
        match self
        {
            Origin::Runtime => "set by code - not saved",
            Origin::Asset => "from the asset - changes against it are saved",
            Origin::Scene => "from the scene file or the editor - saved with the scene",
        }
    }
}
