use egui::{RichText, Ui};

use crate::gui::helper::generic_items::collapse_with_title;
use crate::state::state::State;

use super::super::editor_state::EditorState;

pub fn create_project_settings(editor_state: &mut EditorState, _state: &mut State, ui: &mut Ui)
{
    collapse_with_title(ui, "project_metadata", true, "📋 Project", None, |ui|
    {
        egui::Grid::new("project_metadata_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .min_col_width(60.0)
            .show(ui, |ui|
        {
            ui.label(RichText::new("Name:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_metadata.name).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Version:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_metadata.version).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Author:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_metadata.author).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("URL:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_metadata.url).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("License:").strong());
            ui.add(egui::TextEdit::singleline(&mut editor_state.project_metadata.license).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Build:").strong());
            ui.label(editor_state.project_metadata.build.to_string());
            ui.end_row();
        });
    });

    collapse_with_title(ui, "project_description", true, "📝 Description", None, |ui|
    {
        ui.add(
            egui::TextEdit::multiline(&mut editor_state.project_metadata.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5),
        );
    });
}
