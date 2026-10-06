use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText, TextStyle};
use web_time::Instant;

use crate::gui::editor::editor_state::EditorState;
use crate::helper::concurrency::thread::spawn_thread;
use crate::helper::file::open_with_default_app;
use crate::helper::generic::engine_root;
use crate::state::project::project::ProjectExportDirs;
use crate::state::state::State;

// runs scripts/build.mjs (needs node and the repo next to the editor)
const BUILD_SCRIPT: &str = "scripts/build.mjs";

const COLOR_ERROR: Color32 = Color32::from_rgb(235, 100, 100);
const COLOR_ERROR_DETAIL: Color32 = Color32::from_rgb(200, 140, 140);
const COLOR_WARNING: Color32 = Color32::from_rgb(235, 180, 80);
const COLOR_WARNING_DETAIL: Color32 = Color32::from_rgb(200, 175, 130);
const COLOR_SUCCESS: Color32 = Color32::from_rgb(110, 195, 120);
const COLOR_INFO: Color32 = Color32::from_rgb(120, 170, 235);

#[derive(Clone, Copy, PartialEq)]
pub enum ExportPlatform
{
    Web,
    Windows,
    Linux,
    Mac,
}

impl ExportPlatform
{
    pub const ALL: [ExportPlatform; 4] = [ExportPlatform::Web, ExportPlatform::Windows, ExportPlatform::Linux, ExportPlatform::Mac];

    pub fn name(&self) -> &'static str
    {
        match self
        {
            ExportPlatform::Web => "Web",
            ExportPlatform::Windows => "Windows",
            ExportPlatform::Linux => "Linux",
            ExportPlatform::Mac => "Mac",
        }
    }

    fn arg(&self) -> &'static str
    {
        match self
        {
            ExportPlatform::Web => "web",
            ExportPlatform::Windows => "windows",
            ExportPlatform::Linux => "linux",
            ExportPlatform::Mac => "mac",
        }
    }

    pub fn export_dir<'a>(&self, dirs: &'a mut ProjectExportDirs) -> &'a mut String
    {
        match self
        {
            ExportPlatform::Web => &mut dirs.web,
            ExportPlatform::Windows => &mut dirs.windows,
            ExportPlatform::Linux => &mut dirs.linux,
            ExportPlatform::Mac => &mut dirs.mac,
        }
    }

    // native builds have to run on their own platform
    pub fn supported(&self) -> bool
    {
        match self
        {
            ExportPlatform::Web => true,
            ExportPlatform::Windows => cfg!(target_os = "windows"),
            ExportPlatform::Linux => cfg!(target_os = "linux"),
            ExportPlatform::Mac => cfg!(target_os = "macos"),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ExportStatus
{
    Idle,
    Running,
    Success,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, PartialEq)]
enum LineKind
{
    Normal,
    Info,
    Success,
    Warning,
    WarningDetail,
    Error,
    ErrorDetail,
}

impl LineKind
{
    fn color(&self) -> Option<Color32>
    {
        match self
        {
            LineKind::Normal => None,
            LineKind::Info => Some(COLOR_INFO),
            LineKind::Success => Some(COLOR_SUCCESS),
            LineKind::Warning => Some(COLOR_WARNING),
            LineKind::WarningDetail => Some(COLOR_WARNING_DETAIL),
            LineKind::Error => Some(COLOR_ERROR),
            LineKind::ErrorDetail => Some(COLOR_ERROR_DETAIL),
        }
    }

    fn is_problem(&self) -> bool
    {
        matches!(self, LineKind::Warning | LineKind::WarningDetail | LineKind::Error | LineKind::ErrorDetail)
    }
}

struct LogLine
{
    text: String,
    kind: LineKind,
}

// filled by the reader threads
struct ExportRun
{
    log: Vec<LogLine>,
    status: ExportStatus,
    cancel_requested: bool,
    block: LineKind, // kind of the current compiler diagnostic, its following lines belong to it
    errors: usize,
    warnings: usize,
    duration: Option<Duration>,
}

impl ExportRun
{
    fn new() -> Self
    {
        Self { log: vec![], status: ExportStatus::Idle, cancel_requested: false, block: LineKind::Normal, errors: 0, warnings: 0, duration: None }
    }

    fn push(&mut self, kind: LineKind, text: String)
    {
        self.log.push(LogLine { text, kind });
    }

    fn add_output(&mut self, raw: &str)
    {
        let text = strip_ansi(raw).trim_end().to_string();
        let trimmed = text.trim_start();
        let lower = trimmed.to_lowercase();

        let kind = if is_header(&lower, "error") || lower.contains("panicked at")
        {
            // summary lines repeat errors that were already counted
            if !lower.starts_with("error: could not compile") && !(self.errors > 0 && lower.starts_with("error:") && lower.ends_with(" failed"))
            {
                self.errors += 1;
            }
            self.block = LineKind::ErrorDetail;
            LineKind::Error
        }
        else if is_header(&lower, "warning") || lower.starts_with("warn:")
        {
            if !lower.contains(" generated ")
            {
                self.warnings += 1;
            }
            self.block = LineKind::WarningDetail;
            LineKind::Warning
        }
        else if trimmed.is_empty() || is_cargo_status(&text)
        {
            self.block = LineKind::Normal;
            LineKind::Normal
        }
        else if self.block != LineKind::Normal
        {
            self.block
        }
        else if lower.contains(" build ready")
        {
            LineKind::Success
        }
        else if lower.starts_with("[info]") || lower.starts_with("packaged ")
        {
            LineKind::Info
        }
        else
        {
            LineKind::Normal
        };

        self.push(kind, text);
    }
}

pub struct ExportDialog
{
    pub open: bool,
    pub platform: ExportPlatform,
    pub dev: bool,
    pub save_first: bool,
    pub problems_only: bool,

    status_bar_height: f32,

    run: Arc<Mutex<ExportRun>>,
    pid: Option<u32>,
    started: Option<Instant>,
    target_dir: Option<PathBuf>,
}

impl ExportDialog
{
    pub fn new() -> Self
    {
        Self
        {
            open: false,
            platform: ExportPlatform::Web,
            dev: false,
            save_first: true,
            problems_only: false,
            status_bar_height: 30.0,
            run: Arc::new(Mutex::new(ExportRun::new())),
            pid: None,
            started: None,
            target_dir: None,
        }
    }

    pub fn status(&self) -> ExportStatus
    {
        self.run.lock().unwrap().status
    }

    pub fn show(&mut self, platform: ExportPlatform)
    {
        if self.status() != ExportStatus::Running
        {
            self.platform = platform;
        }
        self.open = true;
    }

    fn start(&mut self, project_path: &str, out_dir: &str)
    {
        let root = repo_root();
        let target_dir = match out_dir.trim()
        {
            "" => root.join("dist").join(self.platform.arg()),
            dir => root.join(dir),
        };

        let mut args = vec![BUILD_SCRIPT.to_string(), format!("--platform={}", self.platform.arg())];
        if self.dev
        {
            args.push("--dev".to_string());
        }
        if !out_dir.trim().is_empty()
        {
            args.push(format!("--out={}", target_dir.display()));
        }
        args.push(project_path.to_string());

        {
            let mut run = self.run.lock().unwrap();
            *run = ExportRun::new();
            run.status = ExportStatus::Running;
            run.push(LineKind::Info, format!("> node {}", args.iter().map(|arg| if arg.contains(' ') { format!("\"{}\"", arg) } else { arg.clone() }).collect::<Vec<_>>().join(" ")));
        }

        self.started = Some(Instant::now());
        self.target_dir = Some(target_dir);
        self.pid = None;

        // scripts/build.mjs drops the build env of the editor itself (RUSTFLAGS of cargo dev, ...)
        let mut command = Command::new("node");
        command.args(&args)
            .current_dir(&root)
            .env("CARGO_TERM_COLOR", "never")
            .env("NO_COLOR", "1")
            .env("FORCE_COLOR", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        // own process group, so cancel also reaches cargo
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let mut child = match command.spawn()
        {
            Ok(child) => child,
            Err(err) =>
            {
                let mut run = self.run.lock().unwrap();
                run.add_output(&format!("error: failed to start node: {} - is node.js installed and in PATH?", err));
                run.status = ExportStatus::Failed;
                return;
            }
        };

        self.pid = Some(child.id());

        let stdout = child.stdout.take().map(|stdout| read_lines(stdout, self.run.clone()));
        let stderr = child.stderr.take().map(|stderr| read_lines(stderr, self.run.clone()));

        let run = self.run.clone();
        let started = Instant::now();
        spawn_thread(move ||
        {
            // all output first, then the final status
            for reader in [stdout, stderr].into_iter().flatten()
            {
                let _ = reader.join();
            }

            let result = child.wait();

            let mut run = run.lock().unwrap();
            run.duration = Some(started.elapsed());
            run.status = if run.cancel_requested
            {
                run.push(LineKind::Warning, "export cancelled".to_string());
                ExportStatus::Cancelled
            }
            else
            {
                match result
                {
                    Ok(status) if status.success() => ExportStatus::Success,
                    Ok(status) =>
                    {
                        run.push(LineKind::Error, format!("export failed ({})", status));
                        ExportStatus::Failed
                    },
                    Err(err) =>
                    {
                        run.push(LineKind::Error, format!("export failed: {}", err));
                        ExportStatus::Failed
                    }
                }
            };
        });
    }

    fn cancel(&mut self)
    {
        let Some(pid) = self.pid else { return; };
        self.run.lock().unwrap().cancel_requested = true;

        // node runs cargo synchronously - the whole process tree has to go
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(0x08000000).output();
        }

        #[cfg(unix)]
        {
            let _ = Command::new("kill").args(["-TERM", &format!("-{}", pid)]).output();
        }
    }
}

fn read_lines<R: Read + Send + 'static>(reader: R, run: Arc<Mutex<ExportRun>>) -> std::thread::JoinHandle<()>
{
    spawn_thread(move ||
    {
        let mut reader = BufReader::new(reader);
        let mut buffer = vec![];

        loop
        {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer)
            {
                Ok(0) | Err(_) => break,
                Ok(_) =>
                {
                    let text = String::from_utf8_lossy(&buffer);
                    let mut run = run.lock().unwrap();

                    // progress output rewrites the line with \r
                    for line in text.trim_end_matches(['\n', '\r']).split('\r')
                    {
                        run.add_output(line);
                    }
                }
            }
        }
    })
}

// the editor is started from the repo root (like data/editor_settings.json), the manifest dir is the fallback
fn repo_root() -> PathBuf
{
    if let Ok(dir) = std::env::current_dir()
    {
        if dir.join(BUILD_SCRIPT).exists()
        {
            return dir;
        }
    }

    engine_root()
}

// "error: ...", "error[E0308]: ..."
fn is_header(lower: &str, word: &str) -> bool
{
    lower.strip_prefix(word).is_some_and(|rest| rest.starts_with(':') || rest.starts_with('['))
}

// "   Compiling rustl ...", "    Finished ..." end a diagnostic
fn is_cargo_status(text: &str) -> bool
{
    let trimmed = text.trim_start();
    let word = trimmed.split(' ').next().unwrap_or("");
    text.len() != trimmed.len() && matches!(word, "Compiling" | "Checking" | "Finished" | "Building" | "Running" | "Fresh" | "Downloading" | "Downloaded" | "Updating" | "Locking" | "Adding" | "Blocking")
}

fn strip_ansi(text: &str) -> String
{
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next()
    {
        if c == '\u{1b}'
        {
            if chars.peek() == Some(&'[')
            {
                chars.next();
                while let Some(c) = chars.next()
                {
                    if ('@'..='~').contains(&c)
                    {
                        break;
                    }
                }
            }
            continue;
        }
        result.push(c);
    }

    result
}

fn format_duration(duration: Duration) -> String
{
    let secs = duration.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn create_modal_export(editor_state: &mut EditorState, state: &mut State, ctx: &egui::Context)
{
    let mut open = editor_state.export.open;
    let mut start = false;
    let mut cancel = false;

    let project_path = editor_state.project_path.clone();
    let dialog = &mut editor_state.export;
    let status = dialog.status();
    let running = status == ExportStatus::Running;

    if running
    {
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    egui::Window::new("Export")
        .default_pos(ctx.content_rect().center() - egui::vec2(380.0, 260.0))
        .default_size(egui::vec2(760.0, 520.0))
        .min_width(520.0)
        .min_height(320.0)
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .show(ctx, |ui|
    {
        // ********** settings **********
        ui.add_enabled_ui(!running, |ui|
        {
            egui::Grid::new("export_settings").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui|
            {
                ui.label("Platform");
                ui.horizontal(|ui|
                {
                    for platform in ExportPlatform::ALL
                    {
                        let response = ui.add_enabled(platform.supported(), egui::Button::selectable(dialog.platform == platform, platform.name()));
                        let response = if platform.supported() { response } else { response.on_disabled_hover_text(format!("{} builds have to run on {} (cross compiling needs its linker and SDK)", platform.name(), platform.name())) };
                        if response.clicked()
                        {
                            dialog.platform = platform;
                        }
                    }
                });
                ui.end_row();

                ui.label("Target folder");
                // buttons first from the right, the text field takes exactly the rest (otherwise the window grows every frame)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                {
                    let out_dir = dialog.platform.export_dir(&mut state.project.export_dirs);

                    if ui.add_enabled(!out_dir.is_empty(), egui::Button::new("✖")).on_hover_text("use dist/").clicked()
                    {
                        out_dir.clear();
                    }

                    if ui.button("📁").on_hover_text("choose folder").clicked()
                    {
                        if let Some(dir) = rfd::FileDialog::new().set_directory(repo_root()).pick_folder()
                        {
                            *out_dir = dir.display().to_string();
                        }
                    }

                    let hint = format!("empty: dist/{}", dialog.platform.arg());
                    ui.add(egui::TextEdit::singleline(out_dir).hint_text(hint).desired_width(ui.available_width()))
                        .on_hover_text("saved per platform with the project, relative paths start at the repo root");
                });
                ui.end_row();

                ui.label("Options");
                ui.horizontal(|ui|
                {
                    ui.checkbox(&mut dialog.dev, "Dev build").on_hover_text("faster build, slower app");
                    ui.checkbox(&mut dialog.save_first, "Save project first").on_hover_text("the export packs the saved project file");
                });
                ui.end_row();

                ui.label("Project");
                match &project_path
                {
                    Some(path) => { ui.add(egui::Label::new(RichText::new(path).weak()).truncate()); },
                    None => { ui.label(RichText::new("not saved yet - it will be saved first").color(COLOR_WARNING)); },
                }
                ui.end_row();
            });
        });

        ui.add_space(6.0);

        let run = dialog.run.lock().unwrap();

        // ********** export button **********
        let can_start = !running && (project_path.is_some() || dialog.save_first);
        let button = egui::Button::new(RichText::new(format!("▶ Export {}", dialog.platform.name())).strong().color(Color32::WHITE))
            .fill(Color32::from_rgb(80, 120, 200))
            .min_size(egui::vec2(120.0, 26.0));
        let hint = if running { "an export is running" } else { "save the project first" };
        if ui.add_enabled(can_start, button).on_disabled_hover_text(hint).clicked()
        {
            start = true;
        }

        ui.add_space(4.0);

        // ********** log **********
        let rows: Vec<&LogLine> = run.log.iter().filter(|line| !dialog.problems_only || line.kind.is_problem()).collect();
        let row_height = ui.text_style_height(&TextStyle::Monospace);
        let margin = 6.0;

        // the rest of the window minus the status bar (its height from the last frame)
        let log_height = (ui.available_height() - dialog.status_bar_height - 2.0 * margin).max(60.0);

        egui::Frame::new().fill(Color32::from_gray(18)).corner_radius(4.0).inner_margin(margin).show(ui, |ui|
        {
            // show_rows adds the item spacing of this ui to every row - without it here the rows end before the bottom
            ui.spacing_mut().item_spacing.y = 0.0;

            egui::ScrollArea::both()
                .id_salt("export_log")
                .auto_shrink([false, false])
                .max_height(log_height)
                .min_scrolled_height(log_height)
                .stick_to_bottom(true)
                .show_rows(ui, row_height, rows.len(), |ui, range|
            {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                for line in &rows[range]
                {
                    let mut text = RichText::new(&line.text).monospace();
                    if let Some(color) = line.kind.color()
                    {
                        text = text.color(color);
                    }
                    if line.kind == LineKind::Error
                    {
                        text = text.strong();
                    }
                    ui.label(text);
                }
            });
        });

        // ********** status bar **********
        let status_bar = ui.vertical(|ui|
        {
            ui.separator();

            ui.horizontal(|ui|
            {
                match run.status
                {
                    ExportStatus::Idle => { ui.label(RichText::new("Ready").weak()); },
                    ExportStatus::Running =>
                    {
                        ui.spinner();
                        let elapsed = dialog.started.map(|started| started.elapsed()).unwrap_or_default();
                        ui.label(format!("Exporting {}... {}", dialog.platform.name(), format_duration(elapsed)));
                    },
                    ExportStatus::Success => { ui.label(RichText::new(format!("✔ Export finished ({})", format_duration(run.duration.unwrap_or_default()))).color(COLOR_SUCCESS).strong()); },
                    ExportStatus::Failed => { ui.label(RichText::new("✖ Export failed").color(COLOR_ERROR).strong()); },
                    ExportStatus::Cancelled => { ui.label(RichText::new("Export cancelled").color(COLOR_WARNING).strong()); },
                }

                if run.status != ExportStatus::Idle
                {
                    ui.separator();

                    let errors = RichText::new(format!("✖ {} errors", run.errors));
                    ui.label(if run.errors > 0 { errors.color(COLOR_ERROR).strong() } else { errors.weak() });

                    let warnings = RichText::new(format!("⚠ {} warnings", run.warnings));
                    ui.label(if run.warnings > 0 { warnings.color(COLOR_WARNING) } else { warnings.weak() });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|
                {
                    if ui.add_enabled(!run.log.is_empty(), egui::Button::new("Copy Log")).clicked()
                    {
                        let text = run.log.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
                        ui.ctx().copy_text(text);
                    }

                    ui.checkbox(&mut dialog.problems_only, "Show only errors & warnings").on_hover_text("hides all other log lines");

                    if running
                    {
                        ui.separator();
                        if ui.button("■ Cancel").clicked()
                        {
                            cancel = true;
                        }
                    }
                    else if let (ExportStatus::Success, Some(dir)) = (run.status, &dialog.target_dir)
                    {
                        ui.separator();
                        if ui.button("📁 Open Folder").on_hover_text(dir.display().to_string()).clicked()
                        {
                            open_with_default_app(dir);
                        }
                    }
                });
            });
        });

        dialog.status_bar_height = status_bar.response.rect.height() + ui.spacing().item_spacing.y;
    });

    editor_state.export.open = open;

    if cancel
    {
        editor_state.export.cancel();
    }

    if start
    {
        start_export(editor_state, state);
    }
}

fn start_export(editor_state: &mut EditorState, state: &mut State)
{
    if editor_state.export.save_first || editor_state.project_path.is_none()
    {
        match crate::gui::editor::editor_project::save_editor_project_with_dialog(editor_state, state, false)
        {
            Some(path) => editor_state.recent_projects.add_and_save(path),
            None =>
            {
                let mut run = editor_state.export.run.lock().unwrap();
                *run = ExportRun::new();
                run.add_output("error: the project was not saved - nothing exported");
                run.status = ExportStatus::Failed;
                return;
            }
        }
    }

    if let Some(path) = editor_state.project_path.clone()
    {
        let out_dir = editor_state.export.platform.export_dir(&mut state.project.export_dirs).clone();
        editor_state.export.start(&path, &out_dir);
    }
}
