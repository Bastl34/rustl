use egui::{Color32, Frame, RichText, ScrollArea, Ui};

use crate::{helper::console_log::{self, LogEntry, LogType}, state::{gui::{editor::editor_state::EditorState, helper::generic_items::label_with_background}, state::State}};
use egui_extras::{Column, TableBuilder};

pub fn create_console_section(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
{
    ui.set_min_height(220.0);

    ui.horizontal_top(|ui|
    {
        create_console_tree(editor_state, state, ui);
        create_console_list(editor_state, state, ui);
    });
}


pub fn create_console_tree(editor_state: &mut EditorState, _state: &mut State, ui: &mut Ui)
{
    ui.scope(|ui|
    {
        ui.set_min_width(100.0);

        ui.vertical(|ui|
        {
            ui.selectable_value(&mut editor_state.log_type, LogType::All, "☰ All");
            ui.selectable_value(&mut editor_state.log_type, LogType::Log, "🗊 Logs");
            ui.selectable_value(&mut editor_state.log_type, LogType::Error, "❌ Errors");
            ui.selectable_value(&mut editor_state.log_type, LogType::Warning, "⚠ Warnings");
            ui.selectable_value(&mut editor_state.log_type, LogType::Success, "✅ Success");
        });
    });
}

pub fn create_console_list(editor_state: &mut EditorState, _state: &mut State, ui: &mut Ui)
{
    let mut error_amount = 0;
    let mut warning_amount = 0;
    let mut success_amount = 0;
    let mut logs_amount = 0;
    let mut amount_all = 0;

    let filtered_logs =
    {
        let logs = console_log::get_mutex().lock().unwrap();
        let logs = &logs.logs;

        // apply filter and get amounts
        logs.iter().filter(|log|
        {
            if log.log_type == LogType::Error { error_amount += 1; }
            if log.log_type == LogType::Warning { warning_amount += 1; }
            if log.log_type == LogType::Success { success_amount += 1; }
            if log.log_type == LogType::Log { logs_amount += 1; }
            amount_all += 1;

            let filter = editor_state.log_filter.to_lowercase();
            if !filter.is_empty() && log.log.to_lowercase().find(filter.as_str()).is_none()
            {
                return false;
            }
            if log.log_type != editor_state.log_type && editor_state.log_type != LogType::All
            {
                return false;
            }
            true
        }).cloned().collect::<Vec<_>>()
    };

    ui.vertical(|ui|
    {
        ui.horizontal(|ui|
        {
            ui.label("🔍");
            ui.add(egui::TextEdit::singleline(&mut editor_state.log_filter).desired_width(100.0));

            ui.checkbox(&mut editor_state.log_auto_scroll, "Auto Scroll");

            ui.separator();

            label_with_background(ui, format!("Total: {}", amount_all).as_str(), Color32::WHITE, Some(Color32::BLACK));

            ui.separator();

            label_with_background(ui, format!("Errors: {}", error_amount).as_str(), Color32::RED, Some(Color32::WHITE));
            label_with_background(ui, format!("Warnings: {}", warning_amount).as_str(), Color32::YELLOW, Some(Color32::BLACK));
            label_with_background(ui, format!("Success: {}", success_amount).as_str(), Color32::GREEN, Some(Color32::BLACK));
            label_with_background(ui, format!("Log: {}", logs_amount).as_str(), Color32::WHITE, Some(Color32::BLACK));
        });

        ui.separator();

        ui.vertical(|ui|
        {
            let available_height = ui.available_height();
            let mut table = TableBuilder::new(ui)
                .id_salt("console_table")
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::auto())
                .column(Column::auto())
                .column(Column::remainder())
                .min_scrolled_height(0.0)
                .max_scroll_height(available_height);

            if editor_state.log_auto_scroll
            {
                if let Some(row_index) = filtered_logs.len().checked_sub(1)
                {
                    table = table.scroll_to_row(row_index, None);
                }
            }

            table.header(20.0, |mut header|
            {
                header.col(|ui| { ui.strong("Date"); });
                header.col(|ui| { ui.strong("Type"); });
                header.col(|ui| { ui.strong("Message"); });
            }).body(|body|
            {
                body.rows(20.0, filtered_logs.len(), |mut row|
                {
                    let log = &filtered_logs[row.index()];

                    let color = match log.log_type
                    {
                        LogType::Error => egui::Color32::RED,
                        LogType::Warning => egui::Color32::YELLOW,
                        LogType::Success => egui::Color32::GREEN,
                        LogType::Log => egui::Color32::WHITE,
                        _ => egui::Color32::GRAY,
                    };

                    row.col(|ui|
                    {
                        ui.colored_label(color, log.timestamp.format("%Y-%m-%d %H:%M:%S").to_string());
                    });
                    row.col(|ui|
                    {
                        let type_str = match log.log_type
                        {
                            LogType::Log => "Log",
                            LogType::Error => "Error",
                            LogType::Success => "Success",
                            LogType::Warning => "Warning",
                            _ => "?",
                        };
                        ui.colored_label(color, type_str);
                    });
                    row.col(|ui|
                    {
                        ui.colored_label(color, &log.log);
                    });
                });
            });
        });
    });


}