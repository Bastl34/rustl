use egui::RichText;

use crate::gui::editor::editor_state::EditorState;

pub fn fit_size(availiable_size: egui::Vec2, requested_size: egui::Vec2) -> egui::Vec2
{
    if requested_size.x <= 0.0 || requested_size.y <= 0.0
    {
        return egui::Vec2::ZERO;
    }
    let scale = (availiable_size.x / requested_size.x).min(availiable_size.y / requested_size.y);
    egui::vec2(requested_size.x * scale, requested_size.y * scale)
}



pub fn rename_hierarchy_item_or_toggle_selection(ui: &mut egui::Ui, toggle_title: RichText, toggle_selection: &mut bool, editor_state: &mut EditorState, item_id: u32, name: String, rename_fn: Box<dyn FnOnce(String)>) -> egui::Response
{
    if editor_state.hierarchy_rename_id == Some(item_id)
    {
        // *** inline rename input ***
        let input_id = egui::Id::new(("rename_input", item_id));
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
            editor_state.hierarchy_rename_id = Some(item_id);
            editor_state.hierarchy_rename_value = name.clone();
        }
        toggle
    }
}