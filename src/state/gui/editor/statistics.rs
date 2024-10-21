use egui::{Ui, Color32, RichText, Stroke};
use egui_plot::{Corner, Legend, Plot, Line, PlotPoints, LineStyle, PlotPoint, Text};

use crate::state::state::State;
use super::editor_state::EditorState;

const CHART_PADDING_FACTOR: f32 = 1.1;

pub fn create_chart(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/plot_demo.rs#L888

    let fps_points: PlotPoints = state.stats.fps_chart.iter().enumerate().map(|(i, value)|
    {
        [
            i as f64,
            *value as f64
        ]
    }).collect();

    let mut color = Color32::GREEN;
    if state.stats.last_fps < 29
    {
        color = Color32::RED;
    }
    else if state.stats.last_fps < 59
    {
        color = Color32::YELLOW;
    }

    let fps = Line::new(fps_points).color(color).stroke(Stroke::new(2.0, color)).style(LineStyle::Solid).name("FPS");

    let legend = Legend::default().position(Corner::LeftTop);

    let mut max_fps = 0;
    for fps in &state.stats.fps_chart
    {
        max_fps = max_fps.max(*fps);
    }

    let fps_upper: f32 = max_fps as f32 * CHART_PADDING_FACTOR;

    let plot = Plot::new("FPS")
        .legend(legend)
        .y_axis_min_width(4.0)
        .show_axes(false)
        .show_grid(true)
        .auto_bounds(egui::Vec2b::new(true, true))
        .include_y(fps_upper)
        .allow_drag(false)
        .allow_zoom(false)
        .y_axis_position(egui_plot::HPlacement::Right)
        .height(120.0);

    plot.show(ui, |plot_ui|
    {
        plot_ui.line(fps);

        // last FPS entry
        let fps = format!("{:.1}", state.stats.last_fps);
        let pos = (state.stats.fps_chart.len() + 5) as f32;
        let text = RichText::new(fps).strong().size(12.0);
        plot_ui.text(Text::new(PlotPoint::new(pos, state.stats.last_fps), text).name("FPS"));
    });
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
    ui.label(format!(" ⚫ fps: {}", state.stats.last_fps));
    ui.label(format!(" ⚫ absolute fps: {}", state.stats.fps_absolute));
    ui.label(format!(" ⚫ frame time: {:.3} ms", state.stats.frame_time));

    ui.label(RichText::new("⚙ Engine").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.stats.engine_update_time));
    ui.label(format!(" ⚫ render time: {:.3} ms", state.stats.engine_render_time));
    ui.label(format!(" ⚫ draw calls: {}", state.stats.draw_calls));
    ui.label(format!(" ⚫ textures: {}", textures));
    ui.label(format!(" ⚫ materials: {}", materials));

    ui.label(RichText::new("✏ Editor").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.stats.egui_update_time));
    ui.label(format!(" ⚫ render time: {:.3} ms", state.stats.egui_render_time));

    ui.label(RichText::new("🗖 App").strong());
    ui.label(format!(" ⚫ update time: {:.3} ms", state.stats.app_update_time));
}