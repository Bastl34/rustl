use serde::{Deserialize, Serialize};

use crate::{component_downcast_mut};
use crate::state::scene::components::mesh::Mesh;
use crate::state::scene::scene::Scene;
use crate::state::state::InputOutput;
use crate::scene_controller_impl_default;
use crate::state::gui::helper::info_box::warn_box;

use super::scene_controller::{SceneController, SceneControllerBase};


#[derive(Serialize, Deserialize)]
pub struct GenericController
{
    base: SceneControllerBase,
}

impl GenericController
{
    pub fn default() -> Self
    {
        GenericController
        {
            base: SceneControllerBase::new("Generic Controller".to_string(), "⚙".to_string()),
        }
    }
}

#[typetag::serde]
impl SceneController for GenericController
{
    scene_controller_impl_default!();

    fn run_after_deserialize(&mut self, _context: &mut crate::state::scene::components::component::DeserializationContext)
    {
    }

    fn cleanup(&mut self)
    {
    }

    fn cleanup_node(&mut self, _node: crate::state::scene::node::NodeItem) -> bool
    {
        false
    }

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, io: &mut InputOutput, _frame_scale: f32) -> bool
    {
        let mut updated = false;

        let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

        for node in all_nodes
        {
            let node = node.read().unwrap();
            if node.skin.len() > 0
            {
                for mesh in node.find_components::<Mesh>()
                {
                    component_downcast_mut!(mesh, Mesh);
                    if mesh.update_skin_bbox_on_animation
                    {
                        let joint_matrices = node.get_joint_transform_vec(true);
                        if let Some(joint_matrices) = joint_matrices
                        {
                            mesh.calc_bbox_skin(&joint_matrices);

                            updated = true;
                        }
                    }
                }
            }
        }

        let cam = scene.cameras.first();
        if let Some(cam) = cam
        {
            if cam.get_data_tracker().changed()
            {
                let (left, right) = cam.get_left_right_ear_positions();

                let mut audio_device = io.audio_device.write().unwrap();
                audio_device.data.get_mut().left_ear_pos = left;
                audio_device.data.get_mut().right_ear_pos = right;

                updated = true;
            }
        }

        updated
    }

    fn ui(&mut self, ui: &mut egui::Ui, _scene: &mut crate::state::scene::scene::Scene)
    {
        ui.label("Features:");
        ui.label("⚫ update skin bbox on each animation");
        ui.label("⚫ update spatial sound camera position (based on first)");

        warn_box(ui, "Its not recommended to remove or stop this.");
    }
}