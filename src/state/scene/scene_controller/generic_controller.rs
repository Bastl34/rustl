use crate::component_downcast_mut;
use crate::state::scene::components::mesh::Mesh;
use crate::state::scene::scene::Scene;
use crate::{input::input_manager::InputManager, scene_controller_impl_default};
use crate::state::gui::helper::info_box::warn_box;

use super::scene_controller::{SceneController, SceneControllerBase};


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

impl SceneController for GenericController
{
    scene_controller_impl_default!();

    fn update(&mut self, scene: &mut crate::state::scene::scene::Scene, _input_manager: &mut InputManager, _frame_scale: f32) -> bool
    {
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
                        }
                    }
                }
            }
        }

        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, scene: &mut crate::state::scene::scene::Scene)
    {
        ui.label("Features:");
        ui.label(" ⚫ update skin bbox on each animation");

        warn_box(ui, "Its not recommended to remove or stop this.");
    }
}