use egui::{Ui, RichText};

use crate::{state::{state::State, scene::{scene::Scene, components::mesh::Mesh}, gui::helper::generic_items::collapse_with_title}, component_downcast};

use super::editor_state::EditorState;

pub fn create_scene_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let scene_id = editor_state.selected_scene_id;

    // no scene selected
    if scene_id.is_none()
    {
        return;
    }

    let scene_id = scene_id.unwrap();
    let scene = state.find_scene_by_id(scene_id);

    if scene.is_none()
    {
        return;
    }

    let scene = scene.unwrap();

    let mut instances_amout = 0;
    let mut meshes_amout = 0;
    let mut vertices_amout = 0;
    let mut indices_amout = 0;

    let all_nodes = Scene::list_all_child_nodes(&scene.nodes);

    for node in &all_nodes
    {
        let node = node.read().unwrap();
        instances_amout += node.instances.get_ref().len();

        let mesh = node.find_component::<Mesh>();
        if let Some(mesh) = mesh
        {
            component_downcast!(mesh, Mesh);

            meshes_amout += 1;
            vertices_amout += mesh.get_data().vertices.len();
            indices_amout += mesh.get_data().indices.len();
        }
    }

    let mut memory_usage = 0.0;
    let mut gpu_memory_usage = 0.0;
    for texture in &scene.textures
    {
        let texture = texture.1.as_ref().read().unwrap();
        let texture = texture.as_ref();
        memory_usage += texture.memory_usage() as f32;
        gpu_memory_usage += texture.gpu_usage() as f32;
    }

    memory_usage = memory_usage / 1024.0 / 1024.0;
    gpu_memory_usage = gpu_memory_usage / 1024.0 / 1024.0;

    // statistics
    collapse_with_title(ui, "scene_info", true, "📈 Info", |ui|
    {
        ui.label(RichText::new("🎬 scene").strong());
        ui.label(format!(" ⚫ nodes: {}", all_nodes.len()));
        ui.label(format!(" ⚫ instances: {}", instances_amout));
        ui.label(format!(" ⚫ materials: {}", scene.materials.len()));
        ui.label(format!(" ⚫ textures: {}", scene.textures.len()));
        ui.label(format!(" ⚫ cameras: {}", scene.cameras.len()));
        ui.label(format!(" ⚫ lights: {}", scene.lights.get_ref().len()));

        ui.label(RichText::new("◼ geometry").strong());
        ui.label(format!(" ⚫ meshes: {}", meshes_amout));
        ui.label(format!(" ⚫ vertices: {}", vertices_amout));
        ui.label(format!(" ⚫ indices: {}", indices_amout));

        ui.label(RichText::new("🖴 RAM memory usage").strong());
        ui.label(format!(" ⚫ textures: {:.2} MB", memory_usage));

        ui.label(RichText::new("🖵 GPU memory usage").strong());
        ui.label(format!(" ⚫ textures: {:.2} MB", gpu_memory_usage));
        ui.label(format!(" ⚫ buffers: TODO"));
    });

    collapse_with_title(ui, "scene_debugging", true, "🐛 Debugging Settings", |ui|
    {
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui|
        {
            if ui.button("save image").clicked()
            {
                state.save_image = true;
            }

            if ui.button("save depth pass image").clicked()
            {
                state.save_depth_pass_image = true;
            }

            if ui.button("save depth buffer image").clicked()
            {
                state.save_depth_buffer_image = true;
            }

            if ui.button("save screenshot").clicked()
            {
                state.save_screenshot = true;
            }
        });
    });
}