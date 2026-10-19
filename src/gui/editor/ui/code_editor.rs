// a simple editor for the files of the project code - small changes without VS Code, a save compiles the code (project_code.rs)
// one window with a tab per file, in the main window or in its own native window (like the assets)

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::text::{ByteIndex, CCursor, CCursorRange, LayoutJob, LayoutSection};
use egui::{Color32, ImageSource, RichText, Stroke, TextFormat, ViewportId};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::gui::editor::editor_state::EditorState;
use crate::gui::editor::project_code::Diagnostic;
use crate::gui::helper::generic_items::tab;

const COLOR_ERROR: Color32 = Color32::from_rgb(235, 100, 100);
const COLOR_WARNING: Color32 = Color32::from_rgb(235, 180, 80);
const COLOR_LINE_NUMBER: Color32 = Color32::from_rgb(110, 110, 120);
const COLOR_ICON: Color32 = Color32::from_rgb(205, 205, 210);

const TEXT_MARGIN: f32 = 4.0;
const INDENT: &str = "    ";
const TOOL_ICON_SIZE: f32 = 20.0;
const STATUS_BAR_HEIGHT: f32 = 24.0;

pub struct CodeFile
{
    pub path: PathBuf,
    canonical: Option<PathBuf>,
    text: String,
    saved_text: String,
    modified: Option<SystemTime>,
    error: Option<String>,
    confirm_close: bool,
    // char index of the cursor - the status bar shows the problem of its line
    cursor: usize,
}

impl CodeFile
{
    fn name(&self) -> String
    {
        self.path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
    }

    fn is_dirty(&self) -> bool
    {
        self.text != self.saved_text
    }

    fn save(&mut self)
    {
        match fs::write(&self.path, &self.text)
        {
            Ok(_) =>
            {
                self.saved_text = self.text.clone();
                self.modified = modified_time(&self.path);
                self.error = None;
            },
            Err(err) => self.error = Some(format!("can not save: {}", err)),
        }
    }

    // changed outside - taken over as long as there are no own changes
    fn reload_if_changed(&mut self)
    {
        let modified = modified_time(&self.path);
        if modified == self.modified || self.is_dirty()
        {
            return;
        }

        if let Ok(text) = fs::read_to_string(&self.path)
        {
            self.text = text.clone();
            self.saved_text = text;
        }
        self.modified = modified;
    }
}

#[derive(Default)]
pub struct CodeEditor
{
    pub files: Vec<CodeFile>,
    pub active: usize,
    pub undocked: bool,
    confirm_close_all: bool,
    window_pos: Option<egui::Pos2>,
    window_size: Option<egui::Vec2>,
}

fn modified_time(path: &Path) -> Option<SystemTime>
{
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

pub fn code_editor_viewport_id() -> ViewportId
{
    ViewportId::from_hash_of("code_editor_window")
}

// a new tab - or the tab of the file if it is open already
pub fn open_inline_editor(editor_state: &mut EditorState, path: &Path)
{
    let editor = &mut editor_state.code_editor;

    if let Some(index) = editor.files.iter().position(|file| file.path == path)
    {
        editor.active = index;
        return;
    }

    match fs::read_to_string(path)
    {
        Ok(text) =>
        {
            editor.files.push(CodeFile
            {
                path: path.to_path_buf(),
                canonical: fs::canonicalize(path).ok(),
                saved_text: text.clone(),
                text,
                modified: modified_time(path),
                error: None,
                confirm_close: false,
                cursor: 0,
            });
            editor.active = editor.files.len() - 1;
        },
        Err(err) => { crate::console_error!("can not open {}: {}", path.display(), err); },
    }
}

pub fn create_code_editor(editor_state: &mut EditorState, ctx: &egui::Context)
{
    if editor_state.code_editor.files.is_empty()
    {
        editor_state.code_editor.undocked = false;
        return;
    }

    let can_undock = editor_state.assets_window_supported;
    let playing = !editor_state.visible;
    let diagnostics = editor_state.project_code.diagnostics.clone();
    let editor = &mut editor_state.code_editor;

    if playing && !(editor.undocked && can_undock)
    {
        return;
    }

    for file in &mut editor.files
    {
        file.reload_if_changed();
    }

    if editor.undocked && can_undock
    {
        let mut builder = egui::ViewportBuilder::default().with_title("Code").with_min_inner_size([420.0, 260.0]);
        if let (Some(pos), Some(size)) = (editor.window_pos, editor.window_size)
        {
            builder = builder.with_position(pos).with_inner_size(size);
        }
        else
        {
            builder = builder.with_inner_size([820.0, 640.0]);
        }

        ctx.show_viewport_immediate(code_editor_viewport_id(), builder, |ui, _class|
        {
            // closing the native window docks the editor back - nothing unsaved gets lost
            if ui.input(|input| input.viewport().close_requested())
            {
                editor.undocked = false;
            }

            if let Some((outer_rect, inner_rect)) = ui.input(|input| input.viewport().outer_rect.zip(input.viewport().inner_rect))
            {
                editor.window_pos = Some(outer_rect.min);
                editor.window_size = Some(inner_rect.size());
            }

            egui::CentralPanel::default().show(ui, |ui|
            {
                // play mode: stays open, but the game has the input
                if playing
                {
                    ui.disable();
                }

                code_editor_ui(ui, editor, &diagnostics, can_undock);
            });
        });
    }
    else
    {
        let mut open = true;
        egui::Window::new("Code")
            .id(egui::Id::unique("code_editor_window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([780.0, 580.0])
            .default_pos(ctx.content_rect().center() - egui::vec2(390.0, 290.0))
            .show(ctx, |ui|
        {
            code_editor_ui(ui, editor, &diagnostics, can_undock);
        });

        // closing with unsaved changes asks first
        if !open
        {
            if editor.files.iter().any(CodeFile::is_dirty)
            {
                editor.confirm_close_all = true;
            }
            else
            {
                editor.files.clear();
            }
        }
    }
}

// flat icon button like in the run mode bar
fn tool_button(ui: &mut egui::Ui, image: ImageSource<'static>, enabled: bool, hover: &str) -> bool
{
    let image = egui::Image::new(image).fit_to_exact_size(egui::vec2(TOOL_ICON_SIZE, TOOL_ICON_SIZE)).tint(COLOR_ICON);
    let button = egui::Button::image(image).frame_when_inactive(false).corner_radius(4);

    ui.add_enabled(enabled, button).on_hover_text(hover).on_disabled_hover_text(hover).clicked()
}

fn code_editor_ui(ui: &mut egui::Ui, editor: &mut CodeEditor, diagnostics: &[Diagnostic], can_undock: bool)
{
    editor.active = editor.active.min(editor.files.len().saturating_sub(1));
    let mut close_tab = None;

    // panels: the text gets the space the window has - a scroll area alone would make the window as high as the screen
    egui::Panel::top("code_editor_head").frame(egui::Frame::NONE).show(ui, |ui|
    {
        // ******************** tools ********************
        ui.horizontal(|ui|
        {
            ui.spacing_mut().button_padding = egui::vec2(4.0, 4.0);

            if let Some(file) = editor.files.get_mut(editor.active)
            {
                if tool_button(ui, egui::include_image!("../../../../resources/icons/save.svg"), true,"Save (Ctrl+S) - the code compiles after saving")
                {
                    file.save();
                }

                if tool_button(ui, egui::include_image!("../../../../resources/icons/revert.svg"), file.is_dirty(), "Revert to the saved file")
                {
                    file.text = file.saved_text.clone();
                }

                ui.separator();
                ui.label(RichText::new(file.path.display().to_string()).weak().small());

                if let Some(error) = &file.error
                {
                    ui.label(RichText::new(error).color(COLOR_ERROR));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                if editor.undocked
                {
                    if ui.button("⬋").on_hover_text("back into the main window").clicked()
                    {
                        editor.undocked = false;
                    }
                }
                else if can_undock && ui.button("⬈").on_hover_text("in its own window").clicked()
                {
                    editor.undocked = true;
                }
            });
        });

        // ******************** tabs ********************
        ui.horizontal(|ui|
        {
            ui.spacing_mut().item_spacing.x = 2.0;

            for (index, file) in editor.files.iter().enumerate()
            {
                let file_diagnostics = diagnostics_of(diagnostics, file);
                let color = if file_diagnostics.iter().any(|diagnostic| diagnostic.error) { Some(COLOR_ERROR) } else if !file_diagnostics.is_empty() { Some(COLOR_WARNING) } else { None };

                let mut label = RichText::new(if file.is_dirty() { format!("{} *", file.name()) } else { file.name() });
                if let Some(color) = color
                {
                    label = label.color(color);
                }

                let result = tab(ui, label, index == editor.active, true);
                if result.clicked
                {
                    editor.active = index;
                }
                if result.icon_clicked
                {
                    close_tab = Some(index);
                }
                result.response.on_hover_text(file.path.display().to_string());
            }
        });

        if editor.confirm_close_all
        {
            let dirty = editor.files.iter().filter(|file| file.is_dirty()).count();
            let message = if dirty == 1 { "1 file has unsaved changes".to_string() } else { format!("{} files have unsaved changes", dirty) };

            match unsaved_bar(ui, &message, "Save All")
            {
                Some(UnsavedChoice::Save) =>
                {
                    editor.files.iter_mut().filter(|file| file.is_dirty()).for_each(CodeFile::save);
                    if editor.files.iter().all(|file| file.error.is_none())
                    {
                        editor.files.clear();
                    }
                    editor.confirm_close_all = false;
                },
                Some(UnsavedChoice::Discard) =>
                {
                    editor.files.clear();
                    editor.confirm_close_all = false;
                },
                Some(UnsavedChoice::Cancel) => editor.confirm_close_all = false,
                None => {},
            }
        }
    });

    // ******************** the active file ********************
    if let Some(file) = editor.files.get_mut(editor.active)
    {
        let file_diagnostics = diagnostics_of(diagnostics, file);
        if file_ui(ui, file, &file_diagnostics)
        {
            close_tab = Some(editor.active);
        }
    }

    if let Some(index) = close_tab
    {
        let file = &mut editor.files[index];

        if file.is_dirty() && !file.confirm_close
        {
            file.confirm_close = true;
            editor.active = index;
        }
        else
        {
            editor.files.remove(index);
            if editor.active > index || editor.active >= editor.files.len()
            {
                editor.active = editor.active.saturating_sub(1);
            }
        }
    }
}

fn diagnostics_of<'a>(diagnostics: &'a [Diagnostic], file: &CodeFile) -> Vec<&'a Diagnostic>
{
    let Some(canonical) = &file.canonical else { return vec![]; };
    diagnostics.iter().filter(|diagnostic| diagnostic.file == *canonical).collect()
}

// the first press of the key with modifiers the filter accepts - taken out of the input, so the text edit does not see it
// (consume_key would also take it with an extra shift or alt)
fn take_key(ui: &egui::Ui, key: egui::Key, accept: impl Fn(egui::Modifiers) -> bool) -> Option<egui::Modifiers>
{
    ui.input_mut(|input|
    {
        let index = input.events.iter().position(|event| matches!(event, egui::Event::Key { key: event_key, pressed: true, modifiers, .. } if *event_key == key && accept(*modifiers)))?;

        match input.events.remove(index)
        {
            egui::Event::Key { modifiers, .. } => Some(modifiers),
            _ => None,
        }
    })
}

enum UnsavedChoice
{
    Save,
    Discard,
    Cancel,
}

// the question before closing with unsaved changes - a bar above the text
fn unsaved_bar(ui: &mut egui::Ui, message: &str, save: &str) -> Option<UnsavedChoice>
{
    let mut choice = None;

    egui::Frame::new()
        .fill(COLOR_WARNING.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, COLOR_WARNING.gamma_multiply(0.45)))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .outer_margin(egui::Margin::symmetric(0, 4))
        .show(ui, |ui|
    {
        ui.horizontal(|ui|
        {
            ui.label(RichText::new("⚠").color(COLOR_WARNING));
            ui.label(RichText::new(message).color(COLOR_WARNING));

            // right_to_left: added in reverse, so it reads "Save | Discard | Cancel"
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
            {
                if ui.button("Cancel").clicked()
                {
                    choice = Some(UnsavedChoice::Cancel);
                }

                if ui.button("Discard").on_hover_text("close without saving").clicked()
                {
                    choice = Some(UnsavedChoice::Discard);
                }

                let save_button = egui::Button::new(RichText::new(save).strong().color(Color32::WHITE)).fill(Color32::from_rgb(80, 120, 200));
                if ui.add(save_button).clicked()
                {
                    choice = Some(UnsavedChoice::Save);
                }
            });
        });
    });

    choice
}

// true: close the tab
fn file_ui(ui: &mut egui::Ui, file: &mut CodeFile, diagnostics: &[&Diagnostic]) -> bool
{
    let mut close = false;
    let text_id = ui.make_persistent_id(("code_editor_text", &file.path));

    if file.confirm_close
    {
        match unsaved_bar(ui, &format!("{} has unsaved changes", file.name()), "Save")
        {
            Some(UnsavedChoice::Save) =>
            {
                file.save();
                close = file.error.is_none();
            },
            Some(UnsavedChoice::Discard) =>
            {
                file.text = file.saved_text.clone();
                close = true;
            },
            Some(UnsavedChoice::Cancel) => file.confirm_close = false,
            None => {},
        }
    }

    // keys the text edit would handle differently - taken before it sees them
    if ui.memory(|memory| memory.has_focus(text_id))
    {
        if take_key(ui, egui::Key::Enter, |modifiers| modifiers.is_none()).is_some()
        {
            new_line_with_indent(ui.ctx(), text_id, &mut file.text);
        }

        // home - and cmd+left on the mac - with shift it selects
        let home = take_key(ui, egui::Key::Home, |modifiers| !modifiers.ctrl && !modifiers.alt && !modifiers.mac_cmd && !modifiers.command)
            .or_else(|| take_key(ui, egui::Key::ArrowLeft, |modifiers| modifiers.mac_cmd && !modifiers.ctrl && !modifiers.alt));

        if let Some(modifiers) = home
        {
            smart_home(ui.ctx(), text_id, &file.text, modifiers.shift);
        }
    }

    let language = file.path.extension().map(|extension| extension.to_string_lossy().to_string()).unwrap_or_default();
    let theme = CodeTheme::from_memory(ui.ctx(), ui.style());

    // no wrapping - long lines scroll like in a code editor, the places of errors and warnings are underlined
    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap_width: f32|
    {
        let mut job = highlight(ui.ctx(), ui.style(), &theme, text.as_str(), &language);
        job.wrap.max_width = f32::INFINITY;

        for diagnostic in diagnostics
        {
            if let Some(range) = diagnostic_range(text.as_str(), diagnostic)
            {
                underline(&mut job, range, if diagnostic.error { COLOR_ERROR } else { COLOR_WARNING });
            }
        }

        ui.fonts_mut(|fonts| fonts.layout_job(job))
    };

    egui::Panel::bottom(("code_editor_status", &file.path)).frame(egui::Frame::NONE).show(ui, |ui|
    {
        status_bar(ui, file, diagnostics, text_id);
    });

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui|
    {
        egui::ScrollArea::both().id_salt(("code_editor_scroll", &file.path)).auto_shrink([false, false]).show(ui, |ui|
        {
            let response = ui.horizontal_top(|ui|
            {
                line_numbers(ui, &file.text, diagnostics);

                ui.add(egui::TextEdit::multiline(&mut file.text)
                    .id(text_id)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .margin(egui::Margin::same(TEXT_MARGIN as i8))
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .desired_rows(30)
                    .layouter(&mut layouter))
            }).inner;

            // egui keeps the keys while the text has the focus - the editor's Ctrl+S (save project) does not see it
            let save = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
            if response.has_focus() && ui.input_mut(|input| input.consume_shortcut(&save))
            {
                file.save();
            }
        });
    });

    if let Some(range) = egui::TextEdit::load_state(ui.ctx(), text_id).and_then(|state| state.cursor.char_range())
    {
        file.cursor = range.primary.index.0;
    }

    close
}

// the problem in the line of the cursor (hover: the complete compiler message), the position and the amount of errors and warnings
fn status_bar(ui: &mut egui::Ui, file: &CodeFile, diagnostics: &[&Diagnostic], text_id: egui::Id)
{
    let (line, column) = line_and_column(&file.text, file.cursor);

    let mut in_line: Vec<&&Diagnostic> = diagnostics.iter().filter(|diagnostic| (diagnostic.line_start..=diagnostic.line_end).contains(&line)).collect();
    in_line.sort_by_key(|diagnostic| !diagnostic.error);

    ui.separator();
    ui.horizontal(|ui|
    {
        ui.set_min_height(STATUS_BAR_HEIGHT);

        if let Some(diagnostic) = in_line.first()
        {
            let (icon, color) = if diagnostic.error { ("❌", COLOR_ERROR) } else { ("⚠", COLOR_WARNING) };
            let more = if in_line.len() > 1 { format!("  (+{})", in_line.len() - 1) } else { String::new() };

            let complete = in_line.iter().map(|diagnostic| diagnostic.rendered.as_str()).collect::<Vec<_>>().join("\n\n");
            ui.add(egui::Label::new(RichText::new(format!("{} {}{}", icon, diagnostic.message, more)).color(color)).truncate())
                .on_hover_ui(|ui| { ui.label(RichText::new(complete).monospace()); });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
        {
            let errors: Vec<&&Diagnostic> = diagnostics.iter().filter(|diagnostic| diagnostic.error).collect();
            let warnings: Vec<&&Diagnostic> = diagnostics.iter().filter(|diagnostic| !diagnostic.error).collect();

            // click: to the next one after the cursor
            for (list, icon, color, name) in [(warnings, "⚠", COLOR_WARNING, "warning"), (errors, "❌", COLOR_ERROR, "error")]
            {
                let text = RichText::new(format!("{} {}", icon, list.len())).color(if list.is_empty() { COLOR_LINE_NUMBER } else { color });
                let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));

                if !list.is_empty() && response.on_hover_text(format!("click: the next {}", name)).clicked()
                {
                    let next = list.iter().find(|diagnostic| diagnostic.line_start > line).or(list.first());
                    if let Some(index) = next.and_then(|diagnostic| char_index(&file.text, diagnostic.line_start, diagnostic.column_start))
                    {
                        set_cursor(ui.ctx(), text_id, index, index);
                        ui.memory_mut(|memory| memory.request_focus(text_id));
                    }
                }
            }

            ui.separator();
            ui.label(RichText::new(format!("Ln {}, Col {}", line, column)).color(COLOR_LINE_NUMBER));
        });
    });
}

// the line numbers next to the text - red/yellow with an error/warning in the line, hover shows the message
fn line_numbers(ui: &mut egui::Ui, text: &str, diagnostics: &[&Diagnostic])
{
    let lines = text.split('\n').count();
    let digits = lines.to_string().len();
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());

    let mut job = LayoutJob::default();
    for line in 1..=lines
    {
        let in_line: Vec<&&Diagnostic> = diagnostics.iter().filter(|diagnostic| (diagnostic.line_start..=diagnostic.line_end).contains(&line)).collect();
        let color = if in_line.iter().any(|diagnostic| diagnostic.error) { COLOR_ERROR } else if !in_line.is_empty() { COLOR_WARNING } else { COLOR_LINE_NUMBER };

        let number = format!("{:>width$}{}", line, if line < lines { "\n" } else { "" }, width = digits);
        job.append(&number, 0.0, TextFormat { font_id: font_id.clone(), color, ..Default::default() });
    }

    ui.vertical(|ui|
    {
        ui.add_space(TEXT_MARGIN);
        let response = ui.add(egui::Label::new(job).sense(egui::Sense::hover()));

        if let Some(pos) = response.hover_pos()
        {
            let row_height = response.rect.height() / lines as f32;
            let line = ((pos.y - response.rect.top()) / row_height) as usize + 1;

            let messages: Vec<String> = diagnostics.iter()
                .filter(|diagnostic| (diagnostic.line_start..=diagnostic.line_end).contains(&line))
                .map(|diagnostic| format!("{} {}", if diagnostic.error { "❌" } else { "⚠" }, diagnostic.message))
                .collect();

            if !messages.is_empty()
            {
                response.on_hover_text(messages.join("\n"));
            }
        }
    });
}

// ******************** text positions ********************

// char index of a 1-based line and column (columns in chars, like rustc)
fn char_index(text: &str, line: usize, column: usize) -> Option<usize>
{
    let mut index = 0;
    for (number, content) in text.split('\n').enumerate()
    {
        let length = content.chars().count();
        if number + 1 == line
        {
            return Some(index + (column.saturating_sub(1)).min(length));
        }
        index += length + 1;
    }
    None
}

// 1-based line and column of a char index
fn line_and_column(text: &str, char_index: usize) -> (usize, usize)
{
    let before: String = text.chars().take(char_index).collect();
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |line| line.chars().count()) + 1;
    (line, column)
}

fn byte_index(text: &str, char_index: usize) -> usize
{
    text.char_indices().nth(char_index).map_or(text.len(), |(byte, _)| byte)
}

fn diagnostic_range(text: &str, diagnostic: &Diagnostic) -> Option<Range<usize>>
{
    let start = char_index(text, diagnostic.line_start, diagnostic.column_start)?;
    let end = char_index(text, diagnostic.line_end, diagnostic.column_end).unwrap_or(start).max(start + 1);
    Some(byte_index(text, start)..byte_index(text, end))
}

// splits the sections of the highlighted code at the range and underlines it
fn underline(job: &mut LayoutJob, range: Range<usize>, color: Color32)
{
    let range = ByteIndex(range.start)..ByteIndex(range.end);
    let mut sections = Vec::with_capacity(job.sections.len() + 2);

    for section in job.sections.drain(..)
    {
        let section_range = section.byte_range.clone();
        if section_range.end <= range.start || section_range.start >= range.end
        {
            sections.push(section);
            continue;
        }

        if section_range.start < range.start
        {
            sections.push(LayoutSection { byte_range: section_range.start..range.start, ..section.clone() });
        }

        let mut marked = section.clone();
        marked.byte_range = section_range.start.max(range.start)..section_range.end.min(range.end);
        marked.format.underline = Stroke::new(1.5, color);
        marked.leading_space = 0.0;
        sections.push(marked);

        if section_range.end > range.end
        {
            sections.push(LayoutSection { byte_range: range.end..section_range.end, leading_space: 0.0, ..section });
        }
    }

    job.sections = sections;
}

fn set_cursor(ctx: &egui::Context, text_id: egui::Id, start: usize, end: usize)
{
    let mut state = egui::TextEdit::load_state(ctx, text_id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(start), CCursor::new(end))));
    state.store(ctx, text_id);
}

// replaces the selection with a line break and the indentation of the current line (one level more after an opening bracket)
fn new_line_with_indent(ctx: &egui::Context, text_id: egui::Id, text: &mut String)
{
    let Some(state) = egui::TextEdit::load_state(ctx, text_id) else { return; };
    let Some(range) = state.cursor.char_range() else { return; };

    let [start, end] = range.sorted_cursors();
    let (start, end) = (start.index.0, end.index.0);
    let (start_byte, end_byte) = (byte_index(text, start), byte_index(text, end));

    let line = text[..start_byte].rsplit('\n').next().unwrap_or_default();
    let mut indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    if line.trim_end().ends_with(['{', '(', '['])
    {
        indent.push_str(INDENT);
    }

    let insert = format!("\n{}", indent);
    text.replace_range(start_byte..end_byte, &insert);

    let cursor = start + insert.chars().count();
    set_cursor(ctx, text_id, cursor, cursor);
}

// home: first to the code after the indentation, again to the start of the line - a line of only indentation goes to its start
// with shift the selection keeps its other end
fn smart_home(ctx: &egui::Context, text_id: egui::Id, text: &str, select: bool)
{
    let Some(mut state) = egui::TextEdit::load_state(ctx, text_id) else { return; };
    let Some(range) = state.cursor.char_range() else { return; };

    let cursor = range.primary.index.0;
    let byte = byte_index(text, cursor);

    let line_start_byte = text[..byte].rfind('\n').map_or(0, |newline| newline + 1);
    let line_end_byte = text[line_start_byte..].find('\n').map_or(text.len(), |newline| line_start_byte + newline);
    let line = &text[line_start_byte..line_end_byte];

    let line_start = text[..line_start_byte].chars().count();
    let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let only_indent = line.trim().is_empty();
    let code_start = line_start + indent;

    let target = if only_indent || cursor == code_start { line_start } else { code_start };

    let target = CCursor::new(target);
    let secondary = if select { range.secondary } else { target };
    state.cursor.set_char_range(Some(CCursorRange { primary: target, secondary, h_pos: None }));
    state.store(ctx, text_id);
}
