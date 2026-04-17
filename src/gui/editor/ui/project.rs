use egui::{RichText, Ui};

use crate::gui::helper::generic_items::collapse_with_title;
use crate::helper::generic::format_duration_secs;
use crate::state::state::State;

use super::super::editor_state::EditorState;

pub fn create_project_settings(editor_state: &mut EditorState, _state: &mut State, ui: &mut Ui)
{
    collapse_with_title(ui, "project_data", true, "📋 Project", None, |ui|
    {
        egui::Grid::new("project_data_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .min_col_width(60.0)
            .show(ui, |ui|
        {
            ui.label(RichText::new("Name:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_data.name).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Version:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_data.version).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Author:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_data.author).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("URL:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_data.url).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("License:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_data.license).desired_width(f32::INFINITY));
            ui.end_row();

            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| ui.label(RichText::new("Path:").strong()));
            ui.add(egui::Label::new(editor_state.project_path.as_ref().map_or_else(|| "None".into(), |p| p.clone())).wrap());
            ui.end_row();

            ui.label(RichText::new("Build:").strong());
            ui.label(editor_state.project_data.build.to_string());
            ui.end_row();

            ui.label(RichText::new("Editing Time:").strong());
            {
                let total_time = editor_state.project_data.editing_time_secs + editor_state.project_session_start.elapsed().as_secs();
                ui.label(format_duration_secs(total_time));
            }
            ui.end_row();
        });
    });

    collapse_with_title(ui, "project_description", true, "📝 Description", None, |ui|
    {
        ui.add(
            egui::TextEdit::multiline(&mut editor_state.project_data.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5),
        );
    });
}
