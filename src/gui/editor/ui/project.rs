use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};

use crate::gui::editor::project_code::{CodeStatus, open_code_file, open_in_vscode};
use crate::helper::file::{join_relative, open_with_default_app};
use crate::helper::console_log::LogSource;
use crate::gui::helper::generic_items::collapse_with_title;
use crate::helper::generic::format_duration_secs;
use crate::state::state::State;

use super::super::editor_state::{BottomPanel, EditorState};
use super::code_editor::open_inline_editor;
use super::helper::ui_helper::{HIERARCHY_BUTTON_SIZE, fit_hierarchy_heading, hierarchy_button_reserve};

const COLOR_CODE_OK: Color32 = Color32::from_rgb(110, 195, 120);
const COLOR_CODE_ERROR: Color32 = Color32::from_rgb(235, 100, 100);
const COLOR_CODE_WARNING: Color32 = Color32::from_rgb(235, 180, 80);

enum CodeAction
{
    Open(PathBuf),
    EditInline(PathBuf),
    EditInlineAll(Vec<PathBuf>),
    OpenFolder(PathBuf),
    VsCode,
    Build,
    ShowLogs,
}

#[derive(Default)]
struct FileTree
{
    dirs: BTreeMap<String, FileTree>,
    files: Vec<(String, String)>,
}

pub fn create_project_settings(editor_state: &mut EditorState, state: &mut State, ui: &mut Ui)
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
            ui.add(egui::TextEdit::singleline(&mut state.project.name).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Version:").strong());
            ui.add(egui::TextEdit::singleline(&mut state.project.version).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("Author:").strong());
            ui.add(egui::TextEdit::singleline(&mut state.project.author).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("URL:").strong());
            ui.add(egui::TextEdit::singleline(&mut state.project.url).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(RichText::new("License:").strong());
            ui.add(egui::TextEdit::singleline(&mut state.project.license).desired_width(f32::INFINITY));
            ui.end_row();

            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| ui.label(RichText::new("Path:").strong()));
            ui.add(egui::Label::new(editor_state.project_path.as_ref().map_or_else(|| "None".into(), |p| p.clone())).wrap());
            ui.end_row();

            ui.label(RichText::new("Build:").strong());
            ui.label(state.project.build.to_string());
            ui.end_row();

            ui.label(RichText::new("Editing Time:").strong());
            {
                let total_time = state.project.editing_time_secs + editor_state.project_session_start.elapsed().as_secs();
                ui.label(format_duration_secs(total_time));
            }
            ui.end_row();
        });
    });

    collapse_with_title(ui, "project_description", true, "📝 Description", None, |ui|
    {
        ui.add(
            egui::TextEdit::multiline(&mut state.project.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5),
        );
    });
}

// ******************** hierarchy ********************

fn build_file_tree(files: &[String]) -> FileTree
{
    let mut root = FileTree::default();

    for file in files
    {
        let mut parts: Vec<&str> = file.split('/').collect();
        let name = parts.pop().unwrap_or_default();

        let mut node = &mut root;
        for part in parts
        {
            node = node.dirs.entry(part.to_string()).or_default();
        }
        node.files.push((name.to_string(), file.clone()));
    }

    root
}

// every file in the tree and its sub folders (paths relative to the code folder)
fn tree_files(tree: &FileTree) -> Vec<&String>
{
    let mut files: Vec<&String> = tree.dirs.values().flat_map(tree_files).collect();
    files.extend(tree.files.iter().map(|(_, path)| path));
    files
}

// the code of the project below the scenes - double click opens a file (folder, code: all files in it), right click has the actions
pub fn create_code_hierarchy(editor_state: &mut EditorState, ui: &mut Ui)
{
    let code = &editor_state.project_code;
    let Some(dir) = code.dir.clone() else { return; };

    let mut actions = vec![];
    let mut selected = code.selected_file.clone();
    let building = code.is_building();

    let ui_id = ui.make_persistent_id("project_code_hierarchy");
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, true).show_header(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            let reserved_right = hierarchy_button_reserve(STATUS_SLOTS);
            let heading = fit_hierarchy_heading(ui, "💻 ", &format!("Code ({})", code.crate_name), "", reserved_right);

            let mut selection = editor_state.bottom == BottomPanel::Console && editor_state.log_source == Some(LogSource::Code) && selected.is_none();
            let toggle = ui.toggle_value(&mut selection, RichText::new(heading).strong());

            if toggle.clicked()
            {
                selected = None;
                actions.push(CodeAction::ShowLogs);
            }

            if toggle.double_clicked()
            {
                actions.push(CodeAction::EditInlineAll(code.files.iter().map(|file| join_relative(&dir, file)).collect()));
            }

            toggle.on_hover_text(format!("{} click: the logs of the code - double click: edit all files inline", dir.display())).context_menu(|ui|
            {
                code_context_menu(ui, &mut actions, &dir, building);
            });

            right_slots(ui, |ui|
            {
                match code.status
                {
                    CodeStatus::Building => { ui.add_sized(HIERARCHY_BUTTON_SIZE, egui::Spinner::new().size(12.0)).on_hover_text("compiling"); },
                    CodeStatus::Ready => status_slot(ui, Some(("✔", COLOR_CODE_OK, "compiled".to_string()))),
                    CodeStatus::Failed => status_slot(ui, Some(("❌", COLOR_CODE_ERROR, "does not compile - click for the logs".to_string()))),
                    CodeStatus::NoCode => {},
                }
            });
        });
    }).body(|ui|
    {
        let tree = build_file_tree(&code.files);
        file_tree_ui(ui, &tree, &dir, "", &code.problems, &mut selected, &mut actions, building);
    });

    editor_state.project_code.selected_file = selected;

    for action in actions
    {
        match action
        {
            CodeAction::Open(path) => open_code_file(&dir, &path),
            CodeAction::EditInline(path) => open_inline_editor(editor_state, &path),
            CodeAction::EditInlineAll(paths) =>
            {
                for path in paths
                {
                    open_inline_editor(editor_state, &path);
                }
            },
            CodeAction::OpenFolder(path) => open_with_default_app(&path),
            CodeAction::VsCode => open_in_vscode(&dir),
            CodeAction::Build => editor_state.project_code.rebuild(),
            CodeAction::ShowLogs =>
            {
                editor_state.bottom = BottomPanel::Console;
                editor_state.bottom_panel_open = true;
                editor_state.log_source = Some(LogSource::Code);
            },
        }
    }
}

// the icons right of the names - fixed slots like the lock and eye buttons of the scene tree
const STATUS_SLOTS: u32 = 2;

// right_to_left from the right edge of the row - the first slot is the rightmost, the same at every depth of the tree
fn right_slots(ui: &mut Ui, add: impl FnOnce(&mut Ui))
{
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
}

fn status_slot(ui: &mut Ui, icon: Option<(&str, Color32, String)>)
{
    match icon
    {
        Some((glyph, color, hover)) => { ui.add_sized(HIERARCHY_BUTTON_SIZE, egui::Label::new(RichText::new(glyph).color(color))).on_hover_text(hover); },
        None => { ui.allocate_exact_size(HIERARCHY_BUTTON_SIZE, egui::Sense::hover()); },
    }
}

// warnings and errors of the last build (folders: of the files in them) - the amount is in the tooltip
fn problems_slots(ui: &mut Ui, (errors, warnings): (usize, usize))
{
    right_slots(ui, |ui|
    {
        status_slot(ui, (errors > 0).then(|| ("❌", COLOR_CODE_ERROR, format!("{} errors", errors))));
        status_slot(ui, (warnings > 0).then(|| ("⚠", COLOR_CODE_WARNING, format!("{} warnings", warnings))));
    });
}

fn problems_color((errors, warnings): (usize, usize)) -> Option<Color32>
{
    if errors > 0 { Some(COLOR_CODE_ERROR) } else if warnings > 0 { Some(COLOR_CODE_WARNING) } else { None }
}

fn file_tree_ui(ui: &mut Ui, tree: &FileTree, dir: &Path, prefix: &str, problems: &HashMap<String, (usize, usize)>, selected: &mut Option<String>, actions: &mut Vec<CodeAction>, building: bool)
{
    for (name, sub_tree) in &tree.dirs
    {
        let path = if prefix.is_empty() { name.clone() } else { format!("{}/{}", prefix, name) };
        let folder = join_relative(dir, &path);

        let prefix_slash = format!("{}/", path);
        let dir_problems = problems.iter().filter(|(file, _)| file.starts_with(&prefix_slash)).fold((0, 0), |sum, (_, count)| (sum.0 + count.0, sum.1 + count.1));

        let ui_id = ui.make_persistent_id(format!("project_code_dir_{}", path));
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui_id, true).show_header(ui, |ui|
        {
            let reserved_right = hierarchy_button_reserve(STATUS_SLOTS);
            let mut label = RichText::new(fit_hierarchy_heading(ui, "📁 ", name, "", reserved_right));
            if let Some(color) = problems_color(dir_problems)
            {
                label = label.color(color);
            }

            // a toggle that never stays on - same look as the files, and it gets the double click
            let mut off = false;
            let response = ui.toggle_value(&mut off, label).on_hover_text("double click: edit all files in it inline");
            if response.double_clicked()
            {
                actions.push(CodeAction::EditInlineAll(tree_files(sub_tree).into_iter().map(|file| join_relative(dir, file)).collect()));
            }

            response.context_menu(|ui|
            {
                code_context_menu(ui, actions, &folder, building);
            });

            problems_slots(ui, dir_problems);
        }).body(|ui|
        {
            file_tree_ui(ui, sub_tree, dir, &path, problems, selected, actions, building);
        });
    }

    for (name, path) in &tree.files
    {
        let file = join_relative(dir, path);
        let mut is_selected = selected.as_deref() == Some(path.as_str());

        let file_problems = problems.get(path).copied().unwrap_or_default();
        let response = ui.horizontal(|ui|
        {
            let reserved_right = hierarchy_button_reserve(STATUS_SLOTS);
            let mut label = RichText::new(fit_hierarchy_heading(ui, "📄 ", name, "", reserved_right));
            if let Some(color) = problems_color(file_problems)
            {
                label = label.color(color);
            }

            let response = ui.toggle_value(&mut is_selected, label).on_hover_text("double click: edit inline - right click: VS Code");
            problems_slots(ui, file_problems);
            response
        }).inner;

        if response.clicked()
        {
            *selected = if is_selected { Some(path.clone()) } else { None };
        }

        if response.double_clicked()
        {
            actions.push(CodeAction::EditInline(file.clone()));
        }

        response.context_menu(|ui|
        {
            if ui.button("✏ Edit Inline").on_hover_text("a simple editor in rustl - for small changes").clicked()
            {
                actions.push(CodeAction::EditInline(file.clone()));
                ui.close();
            }

            if ui.button("📝 Open in VS Code").clicked()
            {
                actions.push(CodeAction::Open(file.clone()));
                ui.close();
            }

            ui.separator();
            code_context_menu(ui, actions, file.parent().unwrap_or(dir), building);
        });
    }
}

fn code_context_menu(ui: &mut Ui, actions: &mut Vec<CodeAction>, folder: &Path, building: bool)
{
    if ui.add_enabled(!building, egui::Button::new("🔨 Build")).clicked()
    {
        actions.push(CodeAction::Build);
        ui.close();
    }

    if ui.button("💻 Open Code Folder in VS Code").on_hover_text("the whole code folder of the project as VS Code workspace").clicked()
    {
        actions.push(CodeAction::VsCode);
        ui.close();
    }

    if ui.button("📂 Open Folder").clicked()
    {
        actions.push(CodeAction::OpenFolder(folder.to_path_buf()));
        ui.close();
    }

    ui.separator();

    if ui.button("🗊 Show Logs").clicked()
    {
        actions.push(CodeAction::ShowLogs);
        ui.close();
    }
}
