use egui::{Color32, RichText};

use crate::{gui::editor::editor_state::EditorState, helper::math::approx_zero, state::scene::{layers::{LAYER_USER_COUNT, LAYER_USER_FIRST_BIT}, utilities::{extras::Extras, origin::Origin}}};

const USER_LAYER_BITS_PER_ROW: u32 = 10;

const ORIGIN_COLUMN_WIDTH: f32 = 48.0;

// the width of the key column: a share of the panel, within limits
fn key_column_width(ui: &egui::Ui) -> f32
{
    (ui.available_width() * 0.35).clamp(70.0, 220.0)
}

// a column of a fixed width in a horizontal row
fn fixed_column<R>(ui: &mut egui::Ui, width: f32, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R
{
    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui|
    {
        ui.set_width(width);
        add_contents(ui)
    }).inner
}

/// "key  value" over the whole width - the key is cut with "…", the value wraps (long paths, ids, ...).
pub fn property_row(ui: &mut egui::Ui, key: impl Into<RichText>, value: impl Into<RichText>)
{
    let key_width = key_column_width(ui);
    ui.horizontal_top(|ui|
    {
        fixed_column(ui, key_width, |ui| ui.add(egui::Label::new(key.into()).truncate()));
        let value_width = ui.available_width();
        fixed_column(ui, value_width, |ui| ui.add(egui::Label::new(value.into()).wrap()));
    });
}

/// The extras of a node or a scene over the whole width: sorted by key, internal ones (leading _) dimmed, long values wrap, origin on the right.
pub fn extras_list(ui: &mut egui::Ui, extras: &Extras)
{
    let mut entries: Vec<_> = extras.iter_with_origin().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    if entries.is_empty()
    {
        ui.label(RichText::new("none").weak());
        return;
    }

    let key_width = key_column_width(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    for (i, (key, value, origin)) in entries.into_iter().enumerate()
    {
        let fill = if i % 2 == 1 { ui.visuals().faint_bg_color } else { Color32::TRANSPARENT };
        egui::Frame::new().fill(fill).inner_margin(egui::Margin::symmetric(2, 1)).show(ui, |ui|
        {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui|
            {
                let key_text = if key.starts_with('_') { RichText::new(key).weak() } else { RichText::new(key) };
                fixed_column(ui, key_width, |ui| ui.add(egui::Label::new(key_text).truncate()));

                let value_width = (ui.available_width() - ORIGIN_COLUMN_WIDTH - ui.spacing().item_spacing.x).max(40.0);
                fixed_column(ui, value_width, |ui| ui.add(egui::Label::new(value.to_string()).wrap()));

                let origin_text = RichText::new(origin.name()).small().weak();
                ui.label(if origin == Origin::Scene { origin_text.color(Color32::from_rgb(110, 170, 255)) } else { origin_text })
                    .on_hover_text(origin.description());
            });
        });
    }
}

pub const HIERARCHY_BUTTON_SIZE: egui::Vec2 = egui::vec2(20.0, 18.0);
pub const HIERARCHY_BUTTON_IMG_SIZE: egui::Vec2 = egui::vec2(18.0, 18.0);
const HIERARCHY_TOGGLE_FRAME_PADDING: f32 = 16.0;
const HIERARCHY_BUTTON_GAP: f32 = 4.0;

/// Returns the pixel budget that should be reserved on the right side of a
/// hierarchy row when `n_buttons` icon buttons (eye/lock/...) follow the heading.
pub fn hierarchy_button_reserve(n_buttons: u32) -> f32
{
    if n_buttons == 0 { 0.0 }
    else { HIERARCHY_BUTTON_SIZE.x * n_buttons as f32 + HIERARCHY_BUTTON_GAP }
}

/// Builds a heading string `"{prefix}{name}{suffix}"` and truncates `name`
/// with `"..."` so the rendered width fits within `ui.available_width() - reserved_right`.
/// `prefix` and `suffix` are always kept (e.g. icon glyph and lock indicator).
pub fn fit_hierarchy_heading(ui: &egui::Ui, prefix: &str, name: &str, suffix: &str, reserved_right: f32) -> String
{
    let max_text_width = (ui.available_width() - reserved_right - HIERARCHY_TOGGLE_FRAME_PADDING).max(20.0);

    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let measure = |s: &str| -> f32
    {
        ui.painter().layout_no_wrap(s.to_string(), font_id.clone(), Color32::WHITE).size().x
    };

    let full = format!("{}{}{}", prefix, name, suffix);
    if name.is_empty() || measure(&full) <= max_text_width
    {
        return full;
    }

    let chars: Vec<char> = name.chars().collect();
    let mut lo: usize = 0;
    let mut hi: usize = chars.len();
    while lo < hi
    {
        let mid = (lo + hi + 1) / 2;
        let truncated: String = chars.iter().take(mid).collect();
        let candidate = format!("{}{}...{}", prefix, truncated, suffix);
        if measure(&candidate) <= max_text_width
        {
            lo = mid;
        }
        else if mid == 0
        {
            break;
        }
        else
        {
            hi = mid - 1;
        }
    }
    let truncated: String = chars.iter().take(lo).collect();
    format!("{}{}...{}", prefix, truncated, suffix)
}

pub fn hierarchy_row_spacer(ui: &mut egui::Ui, reserved_right: f32)
{
    let space = ui.available_width() - reserved_right;
    if space > 0.0 { ui.add_space(space); }
}

pub fn hierarchy_eye_button(ui: &mut egui::Ui, on: bool, hover_text: &str) -> bool
{
    let tint = if on { Color32::LIGHT_GRAY } else { Color32::DARK_GRAY };
    let img = if on
    {
        egui::Image::new(egui::include_image!("../../../../../resources/icons/eye.svg"))
    }
    else
    {
        egui::Image::new(egui::include_image!("../../../../../resources/icons/eye_off.svg"))
    }.fit_to_exact_size(HIERARCHY_BUTTON_IMG_SIZE).tint(tint);

    ui.add(egui::Button::image(img).frame(false).min_size(HIERARCHY_BUTTON_SIZE)).on_hover_text(hover_text).clicked()
}

pub fn hierarchy_lock_button(ui: &mut egui::Ui, locked: bool) -> bool
{
    let tint = if locked { Color32::LIGHT_GRAY } else { Color32::DARK_GRAY };
    let img = if locked
    {
        egui::Image::new(egui::include_image!("../../../../../resources/icons/lock_closed.svg"))
    }
    else
    {
        egui::Image::new(egui::include_image!("../../../../../resources/icons/lock_open.svg"))
    }.fit_to_exact_size(HIERARCHY_BUTTON_IMG_SIZE).tint(tint);

    ui.add(egui::Button::image(img).frame(false).min_size(HIERARCHY_BUTTON_SIZE)).on_hover_text("lock/unlock").clicked()
}

pub fn loading_progress_bar(ui: &mut egui::Ui, progress: f32)
{
    let progress_color = Color32::from_rgb(0, 180, 255);
    let track_color = Color32::from_rgb(20, 30, 40);
    let bar_height = 4.0;

    let progress_frame = egui::Frame::NONE.inner_margin(0.0).outer_margin(0.0).fill(track_color);
    egui::Panel::top("loading_progress_panel")
        .frame(progress_frame)
        .show_separator_line(false)
        .resizable(false)
        .min_size(0.0)
        .max_size(bar_height)
        .exact_size(bar_height)
        .show(ui, |ui|
    {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.style_mut().visuals.selection.bg_fill = progress_color;

        // no progress known (yet): a segment runs through the bar - an empty egui progress bar shows nothing
        if approx_zero(progress)
        {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), bar_height), egui::Sense::hover());
            let width = rect.width();
            let segment = width * 0.25;
            let x = ((ui.input(|input| input.time) as f32 * 0.6).fract() * (width + segment)) - segment;

            let segment_rect = egui::Rect::from_min_max(egui::pos2(rect.left() + x.max(0.0), rect.top()), egui::pos2(rect.left() + (x + segment).min(width), rect.bottom()));
            ui.painter().rect_filled(segment_rect, 0.0, progress_color);
            ui.ctx().request_repaint();
        }
        else
        {
            ui.add(egui::ProgressBar::new(progress).desired_height(bar_height).corner_radius(0).fill(progress_color));
        }
    });
}

pub fn fit_size(availiable_size: egui::Vec2, requested_size: egui::Vec2) -> egui::Vec2
{
    if requested_size.x <= 0.0 || requested_size.y <= 0.0
    {
        return egui::Vec2::ZERO;
    }
    let scale = (availiable_size.x / requested_size.x).min(availiable_size.y / requested_size.y);
    egui::vec2(requested_size.x * scale, requested_size.y * scale)
}


pub fn rename_hierarchy_item_or_toggle_selection(ui: &mut egui::Ui, toggle_title: RichText, toggle_selection: &mut bool, editor_state: &mut EditorState, kind: &str, item_id: u32, name: String, rename_fn: Box<dyn FnOnce(String)>) -> egui::Response
{
    let is_renaming = editor_state.hierarchy_rename_id.as_ref().map_or(false, |(k, i)| k == kind && *i == item_id);

    if is_renaming
    {
        // *** inline rename input ***
        let input_id = egui::Id::unique(("rename_input", kind, item_id));
        let input_wdith = 140.0;
        let resp = ui.add(egui::TextEdit::singleline(&mut editor_state.hierarchy_rename_value).id(input_id).desired_width(input_wdith));
        if !resp.has_focus() && !resp.lost_focus()
        {
            resp.request_focus();
        }

        let commit = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let lost_focus = ui.input(|i| i.key_pressed(egui::Key::Enter)) || resp.lost_focus();

        if (commit || lost_focus) && !cancel
        {
            let new_name = editor_state.hierarchy_rename_value.trim().to_string();
            if !new_name.is_empty()
            {
                rename_fn(new_name);
            }
        }
        if commit || cancel || lost_focus
        {
            editor_state.hierarchy_rename_id = None;
        }

        // return a dummy response that never fires clicked()
        resp
    }
    else
    {
        let toggle = ui.toggle_value(toggle_selection, toggle_title);
        if toggle.double_clicked()
        {
            editor_state.hierarchy_rename_id = Some((kind.to_string(), item_id));
            editor_state.hierarchy_rename_value = name.clone();
        }
        toggle
    }
}

pub fn layer_mask_user_checkboxes(ui: &mut egui::Ui, mask: &mut u32) -> bool
{
    let mut changed = false;

    for row in 0..2u32
    {
        ui.horizontal(|ui|
        {
            for col in 0..USER_LAYER_BITS_PER_ROW
            {
                let user_index = row * USER_LAYER_BITS_PER_ROW + col;
                if user_index >= LAYER_USER_COUNT { break; }

                let bit_index = LAYER_USER_FIRST_BIT + user_index;
                let bit: u32 = 1u32 << bit_index;
                let mut on = (*mask & bit) != 0;

                let res = ui.checkbox(&mut on, "").on_hover_text(format!("Layer {}", bit_index));

                if res.changed()
                {
                    if on { *mask |= bit; } else { *mask &= !bit; }
                    changed = true;
                }
            }
        });
    }

    changed
}