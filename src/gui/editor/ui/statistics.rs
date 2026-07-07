use egui::{Ui, Color32, RichText, Stroke, Frame, Margin, CornerRadius};
use egui_plot::{Plot, Line, PlotPoints, Points, MarkerShape};

use crate::{console_debug, state::state::State};
use super::super::editor_state::EditorState;

const CHART_PADDING_FACTOR: f32 = 1.12;

const FPS_LOW: u32 = 30;
const FPS_MID: u32 = 60;

const COLOR_GOOD: Color32 = Color32::from_rgb(86, 211, 132);
const COLOR_MID: Color32  = Color32::from_rgb(240, 186, 76);
const COLOR_BAD: Color32  = Color32::from_rgb(232, 84, 84);

fn fps_color(fps: u32) -> Color32
{
    if fps < FPS_LOW { COLOR_BAD }
    else if fps < FPS_MID { COLOR_MID }
    else { COLOR_GOOD }
}

fn tint(c: Color32, a: u8) -> Color32
{
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Split a value series into color-homogeneous segments.
/// Boundary sample is included in both neighboring segments so edges connect
/// seamlessly and the color switch happens exactly on the data point.
fn color_segments(values: &[u32]) -> Vec<(Color32, Vec<[f64; 2]>)>
{
    let mut segments: Vec<(Color32, Vec<[f64; 2]>)> = Vec::new();
    if values.is_empty() { return segments; }

    let mut start = 0usize;
    let mut current = fps_color(values[0]);

    let push = |segments: &mut Vec<(Color32, Vec<[f64; 2]>)>, color: Color32, range: std::ops::Range<usize>, values: &[u32]|
    {
        if range.len() < 2 { return; }
        let pts: Vec<[f64; 2]> = range.clone()
            .map(|k| [k as f64, values[k] as f64])
            .collect();
        segments.push((color, pts));
    };

    for i in 1..values.len()
    {
        let c = fps_color(values[i]);
        if c != current
        {
            // include point i (boundary) in the closing segment so the edge
            // from i-1 to i is drawn in the old color
            push(&mut segments, current, start..(i + 1), values);
            start = i; // next segment starts at the boundary point
            current = c;
        }
    }
    push(&mut segments, current, start..values.len(), values);

    segments
}

fn build_lines(values: &[u32], secondary: bool, name: &'static str) -> Vec<Line<'static>>
{
    let mut lines = Vec::new();
    for (color, pts) in color_segments(values)
    {
        let stroke_color = if secondary { tint(color, 170) } else { color };
        let width = if secondary { 1.2 } else { 2.0 };

        let mut line = Line::new(name, PlotPoints::from(pts))
            .color(stroke_color)
            .stroke(Stroke::new(width, stroke_color))
            .name(name);

        if !secondary
        {
            line = line.fill(0.0).fill_alpha(0.10);
        }
        lines.push(line);
    }
    lines
}

fn trend_glyph(delta: i32) -> (&'static str, Color32)
{
    if delta >= 2  { ("⏶", COLOR_GOOD) }
    else if delta <= -2 { ("⏷", COLOR_BAD) }
    else { ("⏹", tint(Color32::WHITE, 130)) }
}

fn value_badge(ui: &mut Ui, value: u32, label: &str, value_size: f32, label_size: f32, trend: Option<i32>)
{
    let color = fps_color(value);
    let bg = tint(color, 28);

    Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, tint(color, 110)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui|
        {
            ui.horizontal(|ui|
            {
                ui.spacing_mut().item_spacing.x = 5.0;
                // monospace + fixed-width pad so the badge does not resize when digit count changes
                ui.label
                (
                    RichText::new(format!("{:>4}", value))
                        .strong()
                        .monospace()
                        .size(value_size)
                        .color(color)
                );
                ui.label(RichText::new(label).size(label_size).color(tint(Color32::WHITE, 200)));

                if let Some(delta) = trend
                {
                    let (glyph, gcolor) = trend_glyph(delta);
                    ui.label
                    (
                        RichText::new(format!("{} {:>+4}", glyph, delta))
                            .monospace()
                            .size(label_size)
                            .color(gcolor)
                    );
                }
            });
        });
}

fn micro_stats(values: &[u32]) -> Option<(u32, u32, u32)>
{
    let nz: Vec<u32> = values.iter().copied().filter(|v| *v > 0).collect();
    if nz.is_empty() { return None; }
    let mn = *nz.iter().min().unwrap();
    let mx = *nz.iter().max().unwrap();
    let av = nz.iter().sum::<u32>() / nz.len() as u32;
    Some((mn, av, mx))
}

fn trend_delta(values: &[u32], current: u32) -> Option<i32>
{
    if values.len() < 4 { return None; }
    let window: Vec<u32> = values.iter().rev().skip(1).take(20).filter(|v| **v > 0).copied().collect();
    if window.is_empty() { return None; }
    let avg = window.iter().sum::<u32>() as i32 / window.len() as i32;
    Some(current as i32 - avg)
}

fn draw_zone(plot_ui: &mut egui_plot::PlotUi, color: Color32, lower: f64, upper: f64, x_min: f64, x_max: f64)
{
    let pts = PlotPoints::from(vec![[x_min, upper], [x_max, upper]]);
    let line = Line::new("", pts)
        .color(color)
        .stroke(Stroke::NONE)
        .fill(lower as f32)
        .fill_alpha(0.06)
        .allow_hover(false);
    plot_ui.line(line);
}

pub fn create_chart(_editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let avg: Vec<u32> = state.stats.fps_average_chart.iter().copied().collect();
    let low: Vec<u32> = state.stats.fps_1_percent_low_chart.iter().copied().collect();

    let last_fps = state.stats.last_fps;
    let last_low = state.stats.last_fps_1_percent_low;

    let trend = trend_delta(&avg, last_fps);
    let stats = micro_stats(&avg);

    let avg_lines = build_lines(&avg, false, "fps");
    let low_lines = build_lines(&low, true, "1% low");

    let marker_points: Vec<(Color32, [f64; 2])> = avg.iter().enumerate()
        .map(|(i, v)| (fps_color(*v), [i as f64, *v as f64]))
        .collect();

    let max_fps = avg.iter().copied().max().unwrap_or(FPS_MID).max(FPS_MID);
    let fps_upper = max_fps as f32 * CHART_PADDING_FACTOR;
    let x_max = (avg.len().saturating_sub(1)) as f64;

    // overall panel width: ~2/3 of available
    let panel_w = (ui.available_width() * 2.0 / 3.0).max(220.0);
    let chart_height = 110.0;

    ui.allocate_ui_with_layout
    (
        egui::vec2(panel_w, 0.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui|
        {
            // ---- header row: badges above the chart ----
            ui.horizontal(|ui|
            {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                value_badge(ui, last_fps, "FPS", 14.0, 9.0, trend);
                value_badge(ui, last_low, "1% LOW", 12.0, 9.0, None);
            });

            ui.add_space(3.0);

            // ---- chart fills full panel width ----
            let plot = Plot::new("FPS")
                .show_axes(egui::Vec2b::new(false, false))
                .show_grid(egui::Vec2b::new(false, true))
                .show_x(true)
                .show_y(true)
                .label_formatter(|name, value|
                {
                    let fps = value.y.round() as i64;
                    if name.is_empty()
                    {
                        format!("{} fps", fps)
                    }
                    else
                    {
                        format!("{}: {} fps", name, fps)
                    }
                })
                .auto_bounds(egui::Vec2b::new(true, true))
                .include_y(0.0)
                .include_y(fps_upper)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .show_background(false)
                .set_margin_fraction(egui::Vec2::new(0.02, 0.05))
                .width(ui.available_width())
                .height(chart_height);

            let response = plot.show(ui, |plot_ui|
            {
                // background threshold zones (red 0-30, yellow 30-60, green 60+)
                draw_zone(plot_ui, COLOR_BAD,  0.0,             FPS_LOW as f64, 0.0, x_max);
                draw_zone(plot_ui, COLOR_MID,  FPS_LOW as f64,  FPS_MID as f64, 0.0, x_max);
                draw_zone(plot_ui, COLOR_GOOD, FPS_MID as f64,  fps_upper as f64, 0.0, x_max);

                // filled colored average line (per-segment)
                for line in avg_lines { plot_ui.line(line); }

                // thinner, lighter 1% low (per-segment, solid)
                for line in low_lines { plot_ui.line(line); }

                // per-point markers on average line (no hover; the line itself owns hover)
                for (color, p) in &marker_points
                {
                    plot_ui.points
                    (
                        Points::new("", PlotPoints::from(vec![*p]))
                            .color(*color)
                            .filled(true)
                            .radius(1.6)
                            .shape(MarkerShape::Circle)
                            .allow_hover(false)
                    );
                }

                // highlight last sample with pulse ring + solid dot
                if let Some(last_idx) = avg.len().checked_sub(1)
                {
                    let lp = [last_idx as f64, last_fps as f64];
                    plot_ui.points
                    (
                        Points::new("", PlotPoints::from(vec![lp]))
                            .color(tint(fps_color(last_fps), 70))
                            .filled(true)
                            .radius(6.5)
                            .shape(MarkerShape::Circle)
                            .allow_hover(false)
                    );
                    plot_ui.points
                    (
                        Points::new("", PlotPoints::from(vec![lp]))
                            .color(fps_color(last_fps))
                            .filled(true)
                            .radius(3.0)
                            .shape(MarkerShape::Circle)
                            .allow_hover(false)
                    );
                }
            });

            // ---- microstats overlayed on the chart's top-left ----
            if let Some((mn, av, mx)) = stats
            {
                let chart_rect = response.response.rect;
                let overlay_origin = chart_rect.min + egui::vec2(6.0, 4.0);
                let overlay_rect = egui::Rect::from_min_size(overlay_origin,egui::vec2(220.0, 20.0),);

                ui.scope_builder
                (
                    egui::UiBuilder::new().max_rect(overlay_rect).layout(egui::Layout::top_down(egui::Align::LEFT)),
                    |ui|
                    {
                        ui.label(
                            RichText::new(format!("min {:>3} · avg {:>3} · max {:>3}", mn, av, mx))
                                .monospace()
                                .size(9.0)
                                .color(tint(Color32::WHITE, 170))
                        );
                    },
                );
            }
        }
    );
}

pub fn create_statistic(_editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let mut materials = 0;
    for scene in &state.scenes
    {
        materials += scene.materials.len();
    }

    // (category, icon, items, separator before the section)
    let mut stats: Vec<(String, String, Vec<String>, bool)> = vec![];

    // info
    let mut info = vec![];
    info.push(format!("fps: {}", state.stats.last_fps));
    info.push(format!("fps 1% low: {}", state.stats.last_fps_1_percent_low));
    info.push(format!("absolute cpu fps: {}", state.stats.fps_cpu_absolute));
    if let Some(fps) = state.stats.fps_gpu_absolute { info.push(format!("absolute gpu fps: {}", fps)); }
    info.push(format!("frame time: {:.3} ms", state.stats.frame_time));
    stats.push(("Info".to_string(), "ℹ".to_string(), info, false));

    // engine
    let mut engine: Vec<_> = vec![];
    engine.push(format!("draw calls: {}", state.stats.draw_calls));
    engine.push(format!("frustum culled objects: {}", state.stats.frustum_culled_objects));
    engine.push(format!("occlusion culled objects: {}", state.stats.occlusion_culled_objects));
    engine.push(format!("shadow views: {}", state.stats.shadow_views));
    engine.push(format!("shadow draw calls: {}", state.stats.shadow_draw_calls));
    engine.push(format!("textures: {}", state.resources.textures.len()));
    engine.push(format!("sounds: {}", state.resources.sound_sources.len()));
    engine.push(format!("materials: {}", materials));
    engine.push(format!("meshes: {}", state.resources.mesh_resources.len()));
    stats.push(("Engine".to_string(), "⚙".to_string(), engine, false));

    // cpu times (frame loop - these run sequentially on the cpu and add up to the frame time)
    // wait = rest of the frame time (waiting for gpu/vsync) - if it is small, the frame is cpu bound
    let cpu_total = state.stats.engine_update_time + state.stats.engine_render_time + state.stats.egui_update_time + state.stats.egui_render_time + state.stats.app_update_time;
    let cpu_wait = (state.stats.frame_time - cpu_total).max(0.0);

    let mut cpu_times: Vec<_> = vec![];
    cpu_times.push(format!("engine update: {:.3} ms", state.stats.engine_update_time));
    cpu_times.push(format!("engine encode: {:.3} ms", state.stats.engine_render_time));
    cpu_times.push(format!("editor update: {:.3} ms", state.stats.egui_update_time));
    cpu_times.push(format!("editor encode: {:.3} ms", state.stats.egui_render_time));
    cpu_times.push(format!("app update: {:.3} ms", state.stats.app_update_time));
    cpu_times.push(format!("total: {:.3} ms", cpu_total));
    cpu_times.push(format!("wait: {:.3} ms", cpu_wait));
    stats.push(("CPU times".to_string(), "⏱".to_string(), cpu_times, true));

    // gpu times (per pass block - the gpu runs in parallel to the cpu and pass windows can overlap,
    // so these do not add up to the frame time)
    // only available if the adapter supports timestamp queries
    let mut gpu_times: Vec<_> = vec![];
    let mut gpu_total = 0.0;
    if let Some(time) = state.stats.gpu_shadow_time { gpu_times.push(format!("shadow pass: {:.3} ms", time)); gpu_total += time; }
    if let Some(time) = state.stats.gpu_depth_time { gpu_times.push(format!("depth pass: {:.3} ms", time)); gpu_total += time; }
    if let Some(time) = state.stats.gpu_color_time { gpu_times.push(format!("color pass: {:.3} ms", time)); gpu_total += time; }
    if let Some(time) = state.stats.gpu_hzb_time { gpu_times.push(format!("hzb culling: {:.3} ms", time)); gpu_total += time; }
    if let Some(time) = state.stats.gpu_egui_time { gpu_times.push(format!("egui pass: {:.3} ms", time)); gpu_total += time; }

    if !gpu_times.is_empty()
    {
        gpu_times.push(format!("total: {:.3} ms", gpu_total));
        stats.push(("GPU times".to_string(), "💻".to_string(), gpu_times, true));
    }

    for (stat_category, stat_icon, stat_items, separator) in &stats
    {
        if *separator
        {
            ui.separator();
        }

        ui.label(RichText::new(format!("{} {}", stat_icon, stat_category)).strong());

        for stat_item in stat_items
        {
            ui.label(format!(" ⚫ {}", stat_item));
        }
    }

    // debug print
    ui.separator();
    ui.horizontal(|ui|
    {
        if ui.button("🐛 Output Stats").clicked()
        {
            console_debug!("Output Stats");
            for (stat_category, _, stat_items, _) in stats
            {
                console_debug!(format!("{}: ",stat_category));

                for stat_item in stat_items
                {
                    console_debug!(format!(" - {}", stat_item));
                }
            }
        }
    });
}
