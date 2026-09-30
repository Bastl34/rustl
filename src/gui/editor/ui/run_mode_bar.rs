use egui::{Color32, CornerRadius, Frame, ImageSource, Margin, Stroke, Ui};

use crate::gui::editor::editor_state::EditorState;
use crate::state::state::{RunMode, State};

// same button size as the tool row (20px icon + 4px padding)
const ICON_SIZE: f32 = 20.0;
const BUTTON_PADDING: egui::Vec2 = egui::vec2(4.0, 4.0);
const BAR_MARGIN: i8 = 2;

// initial estimate only (icon + button padding + bar margin + 1px stroke on both sides), the real height is measured
pub const RUN_MODE_BAR_HEIGHT: f32 = ICON_SIZE + BUTTON_PADDING.y * 2.0 + BAR_MARGIN as f32 * 2.0 + 2.0;
pub const RUN_MODE_BAR_V_PADDING: f32 = 4.0;

const BAR_FILL: Color32 = Color32::from_rgb(24, 24, 27);
const BAR_STROKE: Color32 = Color32::from_rgb(55, 55, 62);
const ACTIVE_FILL: Color32 = Color32::from_rgb(55, 55, 64);

const ICON_COLOR: Color32 = Color32::from_rgb(205, 205, 210);
const PLAY_COLOR: Color32 = Color32::from_rgb(90, 200, 110);
const SIMULATE_COLOR: Color32 = Color32::from_rgb(110, 175, 240);
const STOP_COLOR: Color32 = Color32::from_rgb(230, 100, 100);
const ENGINE_OFF_COLOR: Color32 = Color32::from_rgb(235, 160, 70);

// flat icon button: a background only on hover or while active
fn icon_button(ui: &mut Ui, image: ImageSource<'static>, tint: Color32, active: bool, enabled: bool, hover: &str) -> bool
{
    let img = egui::Image::new(image).fit_to_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE)).tint(tint);

    let mut btn = egui::Button::image(img)
        .frame(true)
        .frame_when_inactive(active)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(4));

    if active
    {
        btn = btn.fill(ACTIVE_FILL);
    }

    ui.add_enabled(enabled, btn).on_hover_text(hover).clicked()
}

fn bar_separator(ui: &mut Ui)
{
    ui.add(egui::Separator::default().spacing(6.0).shrink(3.0));
}

pub fn create_run_mode_bar(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    let run_mode = state.run_mode;
    let running = run_mode.is_running();
    let updating = run_mode.updates_engine();
    let paused = state.pause;
    let fullscreen = *state.rendering.fullscreen.get_ref();

    let mut picked_run_mode: Option<(RunMode, bool)> = None;
    let mut stop = false;
    let mut toggle_pause = false;
    let mut toggle_engine = false;
    let mut toggle_fullscreen = false;

    Frame::new()
        .fill(BAR_FILL)
        .stroke(Stroke::new(1.0, BAR_STROKE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(BAR_MARGIN))
        .show(ui, |ui|
    {
        // the parent layout is right to left, so the controls are added from right to left
        ui.horizontal(|ui|
        {
            ui.spacing_mut().button_padding = BUTTON_PADDING;
            ui.spacing_mut().item_spacing.x = 1.0;

            // egui derives the button margin from expansion and stroke width per state, so zero them to keep the size fixed on hover
            let widgets = &mut ui.visuals_mut().widgets;
            for visuals in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open]
            {
                visuals.expansion = 0.0;
                visuals.bg_stroke = Stroke::NONE;
            }

            // more run options
            {
                let img = egui::Image::new(egui::include_image!("../../../../resources/icons/more.svg")).fit_to_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE)).tint(ICON_COLOR);
                let btn = egui::Button::image(img).frame(true).frame_when_inactive(false).stroke(Stroke::NONE).corner_radius(CornerRadius::same(4));

                let (response, _) = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui|
                {
                    if ui.button("Play (Fullscreen)").clicked() { picked_run_mode = Some((RunMode::Play, true)); }
                    if ui.button("Simulate (Fullscreen)").clicked() { picked_run_mode = Some((RunMode::Simulate, true)); }
                });

                response.on_hover_text("More run options");
            }

            if icon_button(ui, egui::include_image!("../../../../resources/icons/fullscreen.svg"), ICON_COLOR, fullscreen, true, "Fullscreen")
            {
                toggle_fullscreen = true;
            }

            bar_separator(ui);

            let stop_tint = if running { STOP_COLOR } else { ICON_COLOR };
            if icon_button(ui, egui::include_image!("../../../../resources/icons/stop.svg"), stop_tint, false, running, "Stop (Esc) - back to edit, resets every dynamic object")
            {
                stop = true;
            }

            if icon_button(ui, egui::include_image!("../../../../resources/icons/pause.svg"), ICON_COLOR, paused, running, "Pause (P) - keeps everything where it is")
            {
                toggle_pause = true;
            }

            if icon_button(ui, egui::include_image!("../../../../resources/icons/simulate.svg"), SIMULATE_COLOR, run_mode == RunMode::Simulate, true, "Simulate (Ctrl+T, hold Shift for fullscreen) - physics runs, the editor stays open") && run_mode != RunMode::Simulate
            {
                picked_run_mode = Some((RunMode::Simulate, false));
            }

            if icon_button(ui, egui::include_image!("../../../../resources/icons/run.svg"), PLAY_COLOR, run_mode == RunMode::Play, true, "Play (Ctrl+R, hold Shift for fullscreen)") && run_mode != RunMode::Play
            {
                picked_run_mode = Some((RunMode::Play, false));
            }

            bar_separator(ui);

            // engine stop - nothing updates at all while this is off
            let engine_tint = if updating { ICON_COLOR } else { ENGINE_OFF_COLOR };
            if icon_button(ui, egui::include_image!("../../../../resources/icons/engine.svg"), engine_tint, !updating, true, "Engine update on/off")
            {
                toggle_engine = true;
            }
        });
    });

    if toggle_engine
    {
        let run_mode = if updating { RunMode::Stopped } else { RunMode::Edit };
        editor_state.set_run_mode(state, run_mode, false);
    }

    if toggle_fullscreen
    {
        state.rendering.fullscreen.set(!fullscreen);
    }

    if toggle_pause
    {
        editor_state.set_paused(state, !paused);
    }

    if stop
    {
        editor_state.set_run_mode(state, RunMode::Edit, false);
    }
    else if let Some((run_mode, fullscreen)) = picked_run_mode
    {
        editor_state.set_run_mode(state, run_mode, fullscreen);
    }
}
