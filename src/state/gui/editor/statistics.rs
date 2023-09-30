use egui::{Ui, Color32, RichText};
use egui_plot::{BarChart, Bar, Corner, Legend, Plot};

use crate::state::state::State;
use super::editor_state::EditorState;

pub fn create_chart(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/plot_demo.rs#L888

    let chart = BarChart::new
    (
        state.fps_chart.iter().enumerate().map(|(i, value)|
        {
            //Bar::new((i - FPS_CHART_VALUES) as f64, *value as f64).width(0.05)
            Bar::new(i as f64, *value as f64).width(0.05)
        }).collect(),
    )
    .color(Color32::WHITE)
    .name("FPS");

    let legend = Legend::default().position(Corner::LeftTop);

    Plot::new("FPS")
        .legend(legend)
        .clamp_grid(true)
        .y_axis_width(4)
        .y_axis_position(egui_plot::HPlacement::Right)
        .allow_zoom(false)
        .height(120.0)
        .allow_drag(false)
        .show(ui, |plot_ui| plot_ui.bar_chart(chart));
}

pub fn create_statistic(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let mut textures = 0;
    let mut materials = 0;
    for scene in &state.scenes
    {
        textures += scene.textures.len();
        materials += scene.materials.len();
    }

    ui.label(RichText::new("ℹ Info").strong());
    ui.label(format!(" ⚫ fps: {}", state.last_fps));
    ui.label(format!(" ⚫ absolute fps: {}", state.fps_absolute));
    ui.label(format!(" ⚫ frame time: {:.3} ms", state.frame_time));

    ui.label(RichText::new("⚙ Engine").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.engine_update_time));
    ui.label(format!(" ⚫ render time: {:.3} ms", state.engine_render_time));
    ui.label(format!(" ⚫ draw calls: {}", state.draw_calls));
    ui.label(format!(" ⚫ textures: {}", textures));
    ui.label(format!(" ⚫ materials: {}", materials));

    ui.label(RichText::new("✏ Editor").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.egui_update_time));
    ui.label(format!(" ⚫ render time: {:.3} ms", state.egui_render_time));

    ui.label(RichText::new("🗖 App").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.app_update_time));
}