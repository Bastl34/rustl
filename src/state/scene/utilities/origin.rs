use serde::{Deserialize, Serialize};

// where an extra or a tag came from - the editor saves only the scene ones into the scene file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Origin
{
    // set by code (engine, editor, app) - not saved, not editable in the editor
    #[default]
    Runtime,
    // read from the asset (glTF extras) - stays in the asset
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
            Origin::Asset => "from the asset (glTF extras) - stays in the asset",
            Origin::Scene => "from the scene file or the editor - saved with the scene",
        }
    }
}
