use egui::{Color32, RichText};

use crate::{gui::editor::editor_state::EditorState, state::scene::layers::{LAYER_USER_COUNT, LAYER_USER_FIRST_BIT}};

const USER_LAYER_BITS_PER_ROW: u32 = 10;

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
        let input_id = egui::Id::new(("rename_input", kind, item_id));
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