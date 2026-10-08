//! The code of a project: a crate in <project folder>/code, a workspace of its own (the project can be anywhere).
//! The editor (engine as shared library, scripts/dev.mjs) builds it in the background as shared library
//! and loads it when Play starts. Exports link it into the game (scripts/build.mjs).

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use web_time::Instant;

use crate::helper::console_log::{LogSource, LogType, log_from};
use crate::helper::file::{normalize_path_separators, open_with_default_app, real_path, relative_path};
use crate::helper::generic::engine_root;
use crate::interface::app::App;
use crate::{console_error, console_success};

pub const CODE_DIR: &str = "code";

const SCAN_INTERVAL: Duration = Duration::from_millis(1000);
const BUILD_OUTPUT_MAX_LINES: usize = 400;

// the files of a new project code crate - __crate__ and __root__ are replaced (valid names, so the templates stay valid rust and toml)
const TEMPLATE_CARGO_TOML: &str = include_str!("../../../templates/project_code/Cargo.toml");
const TEMPLATE_APP_RS: &str = include_str!("../../../templates/project_code/src/app.rs");
const TEMPLATE_MAIN_RS: &str = include_str!("../../../templates/project_code/src/main.rs");
const TEMPLATE_CARGO_CONFIG: &str = include_str!("../../../templates/project_code/.cargo/config.toml");

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodeStatus
{
    NoCode,
    Building,
    Ready,
    Failed,
}

struct BuildResult
{
    generation: u64,
    fingerprint: u64,
    success: bool,
    library: Option<PathBuf>,
    output: Vec<String>,
    diagnostics: Vec<Diagnostic>,
}

// an error or warning of the last build at a place in a file - the code editor marks it
#[derive(Clone, Debug)]
pub struct Diagnostic
{
    pub file: PathBuf,
    pub error: bool,
    pub message: String,
    // the complete compiler output of it
    pub rendered: String,
    // 1-based, columns in chars
    pub line_start: usize,
    pub column_start: usize,
    pub line_end: usize,
    pub column_end: usize,
}

pub struct ProjectCode
{
    pub dir: Option<PathBuf>,
    pub crate_name: String,
    pub files: Vec<String>,
    pub status: CodeStatus,
    pub output: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    // errors and warnings per file (path relative to the code folder, like files) - the hierarchy marks them
    pub problems: std::collections::HashMap<String, (usize, usize)>,
    pub build_started: Option<Instant>,
    pub last_build_time: Option<Duration>,
    pub selected_file: Option<String>,

    project_path: Option<String>,
    scanned: Option<Instant>,
    fingerprint: u64,
    built_fingerprint: Option<u64>,
    force_build: bool,

    generation: u64,
    build: Option<Receiver<BuildResult>>,
    library: Option<PathBuf>,

    loader: hot::Loader,
}

impl ProjectCode
{
    pub fn new() -> ProjectCode
    {
        ProjectCode
        {
            dir: None,
            crate_name: String::new(),
            files: vec![],
            status: CodeStatus::NoCode,
            output: vec![],
            diagnostics: vec![],
            problems: std::collections::HashMap::new(),
            build_started: None,
            last_build_time: None,
            selected_file: None,

            project_path: None,
            scanned: None,
            fingerprint: 0,
            built_fingerprint: None,
            force_build: false,

            generation: 0,
            build: None,
            library: None,

            loader: hot::Loader::default(),
        }
    }

    pub fn has_code(&self) -> bool
    {
        self.dir.is_some()
    }

    pub fn is_building(&self) -> bool
    {
        self.status == CodeStatus::Building
    }

    // why Play has to wait - None: Play can start
    pub fn play_blocked(&self) -> Option<&'static str>
    {
        match self.status
        {
            CodeStatus::Building => Some("the project code is compiling"),
            CodeStatus::Failed => Some("the project code does not compile - see the console (source: Code)"),
            _ => None,
        }
    }

    pub fn rebuild(&mut self)
    {
        self.force_build = true;
        self.scanned = None;
    }

    // every frame: follows the project, scans the sources and starts a build on changes
    pub fn update(&mut self, project_path: Option<&str>)
    {
        if self.project_path.as_deref() != project_path
        {
            self.set_project(project_path);
        }

        self.receive_build();

        let Some(dir) = self.dir.clone() else { return; };

        if self.scanned.is_some_and(|scanned| scanned.elapsed() < SCAN_INTERVAL)
        {
            return;
        }
        self.scanned = Some(Instant::now());

        let (files, fingerprint) = scan_sources(&dir);
        self.files = files;
        self.fingerprint = fingerprint;

        let changed = self.built_fingerprint != Some(self.fingerprint);
        if self.build.is_none() && (changed || self.force_build)
        {
            self.force_build = false;
            self.start_build(dir);
        }
    }

    fn set_project(&mut self, project_path: Option<&str>)
    {
        self.project_path = project_path.map(str::to_string);
        self.generation += 1;
        self.build = None;
        self.library = None;
        self.files.clear();
        self.output.clear();
        self.built_fingerprint = None;
        self.scanned = None;
        self.build_started = None;

        // the real spelling: cargo resolves the relative engine path from it - another spelling (h: vs H:) would be another engine build
        self.dir = project_path.map(|path| real_path(&code_dir(Path::new(path)))).filter(|dir| dir.join("Cargo.toml").exists());
        self.crate_name = self.dir.as_ref().and_then(|dir| read_crate_name(&dir.join("Cargo.toml"))).unwrap_or_default();
        self.status = if self.dir.is_some() { CodeStatus::Building } else { CodeStatus::NoCode };

    }

    fn start_build(&mut self, dir: PathBuf)
    {
        let (sender, receiver) = channel();
        self.build = Some(receiver);
        self.status = CodeStatus::Building;
        self.build_started = Some(Instant::now());

        let generation = self.generation;
        let fingerprint = self.fingerprint;
        let crate_name = self.crate_name.clone();
        let project_path = self.project_path.clone().unwrap_or_default();

        log_from(&format!("compiling {}...", crate_name), LogType::Log, LogSource::Code);

        let spawned = std::thread::Builder::new().name("project code build".to_string()).spawn(move ||
        {
            let mut diagnostics = vec![];
            let (success, library, output) = build(&project_path, &dir, &crate_name, &mut diagnostics);
            let _ = sender.send(BuildResult { generation, fingerprint, success, library, output, diagnostics });
        });

        if let Err(err) = spawned
        {
            console_error!("project code: can not start the build: {}", err);
            self.build = None;
            self.status = CodeStatus::Failed;
        }
    }

    fn receive_build(&mut self)
    {
        let Some(receiver) = &self.build else { return; };
        let Ok(result) = receiver.try_recv() else { return; };
        self.build = None;

        if result.generation != self.generation
        {
            return;
        }

        self.built_fingerprint = Some(result.fingerprint);
        self.output = result.output;
        self.diagnostics = result.diagnostics;
        self.problems = problems_per_file(self.dir.as_deref(), &self.diagnostics);
        self.last_build_time = self.build_started.map(|started| started.elapsed());

        log_build_output(&self.output);
        let secs = self.last_build_time.map_or(0.0, |time| time.as_secs_f32());

        if result.success && result.library.is_some()
        {
            self.library = result.library;
            self.status = CodeStatus::Ready;
            log_from(&format!("{} compiled ({:.1}s)", self.crate_name, secs), LogType::Success, LogSource::Code);
        }
        else
        {
            self.status = CodeStatus::Failed;
            log_from(&format!("{} does not compile ({:.1}s)", self.crate_name, secs), LogType::Error, LogSource::Code);
        }
    }

    // the app of the latest build - loads the shared library if it changed
    pub fn create_app(&mut self) -> Option<Box<dyn App>>
    {
        if self.status != CodeStatus::Ready
        {
            return None;
        }

        let library = self.library.clone()?;

        match self.loader.factory(&library)
        {
            Ok(factory) => Some(factory()),
            Err(err) =>
            {
                console_error!("project code: can not load {}: {}", library.display(), err);
                None
            },
        }
    }
}

// ******************** build ********************

// one console line per output line - the lines of a diagnostic get the type of its header, so the type filter keeps them together
fn log_build_output(output: &[String])
{
    let mut log_type = LogType::Log;

    for line in output
    {
        if line.trim().is_empty()
        {
            log_type = LogType::Log;
            continue;
        }

        if line.starts_with("error")
        {
            log_type = LogType::Error;
        }
        else if line.starts_with("warning")
        {
            log_type = LogType::Warning;
        }

        log_from(line, log_type.clone(), LogSource::Code);
    }
}

// the same profile and target dir as the running editor - the engine library is shared, it must not be built again
fn editor_build_dirs() -> Option<(PathBuf, String)>
{
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?;
    let target_dir = profile_dir.parent()?.to_path_buf();

    let profile = match profile_dir.file_name()?.to_string_lossy().as_ref()
    {
        "debug" => "dev".to_string(),
        name => name.to_string(),
    };

    Some((target_dir, profile))
}

// the minimal cargo config of a project (templates/project_code/.cargo/config.toml): its lock in .cargo, its builds in the engine
// for every cargo in the code folder (rust-analyzer, wasm-pack, by hand) - only written when it is missing, the user can add to it
pub fn ensure_project_config(dir: &Path) -> Result<(), String>
{
    let file = dir.join(".cargo").join("config.toml");
    if file.exists()
    {
        return Ok(());
    }

    let content = TEMPLATE_CARGO_CONFIG.replace("__root__", &engine_path_from(dir));
    fs::create_dir_all(file.parent().unwrap()).map_err(|err| format!("{}: {}", file.display(), err))?;
    fs::write(&file, content).map_err(|err| format!("{}: {}", file.display(), err))
}

// scripts/build.mjs --editor-library builds it (the same engine args, lock and env as the exports) - its stdout are the json messages of cargo
fn build(project_path: &str, dir: &Path, crate_name: &str, diagnostics: &mut Vec<Diagnostic>) -> (bool, Option<PathBuf>, Vec<String>)
{
    let mut output = vec![];

    if let Err(err) = ensure_project_config(dir)
    {
        output.push(format!("error: {}", err));
        return (false, None, output);
    }

    let Some((target_dir, profile)) = editor_build_dirs() else
    {
        output.push("error: the target dir of the editor is unknown".to_string());
        return (false, None, output);
    };

    // a shared library only for the editor - exports build the crate as rlib into the game
    let root = engine_root();
    let mut command = Command::new("node");
    command.current_dir(&root)
        .arg(root.join("scripts").join("build.mjs"))
        .arg("--editor-library")
        .arg(format!("--profile={}", profile))
        .arg(format!("--target-dir={}", target_dir.display()))
        .arg(project_path);

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let result = match command.output()
    {
        Ok(result) => result,
        Err(err) =>
        {
            output.push(format!("error: can not start node (scripts/build.mjs): {} - is node.js installed and in PATH?", err));
            return (false, None, output);
        },
    };

    // stdout: json messages - only the diagnostics of the project, cargo replays the cached warnings of the engine too
    let target_name = crate_name.replace('-', "_");
    let mut library = None;

    for line in String::from_utf8_lossy(&result.stdout).lines()
    {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else { continue; };

        if message["target"]["name"] != target_name.as_str()
        {
            continue;
        }

        if message["reason"] == "compiler-message"
        {
            if let Some(rendered) = message["message"]["rendered"].as_str()
            {
                output.extend(rendered.lines().map(str::to_string));
            }

            diagnostics.extend(parse_diagnostic(dir, &message["message"]));
        }
        else if message["reason"] == "compiler-artifact"
        {
            let filenames = message["filenames"].as_array().cloned().unwrap_or_default();
            library = filenames.iter()
                .filter_map(|file| file.as_str())
                .find(|file| file.ends_with(&format!(".{}", std::env::consts::DLL_EXTENSION)))
                .map(PathBuf::from)
                .or(library);
        }
    }


    // stderr: the errors of cargo itself (manifest, locked files, ...)
    const PROGRESS: [&str; 11] = ["Compiling", "Checking", "Blocking", "Fresh", "Finished", "Building", "Locking", "Adding", "Updating", "Downloading", "Downloaded"];
    for line in String::from_utf8_lossy(&result.stderr).lines()
    {
        let trimmed = line.trim_start();
        let other_crate_summary = trimmed.starts_with("warning: `") && trimmed.contains(" generated ");

        if trimmed.is_empty() || other_crate_summary || PROGRESS.iter().any(|word| trimmed.starts_with(word))
        {
            continue;
        }
        output.push(line.to_string());
    }

    if output.len() > BUILD_OUTPUT_MAX_LINES
    {
        output.drain(..output.len() - BUILD_OUTPUT_MAX_LINES);
    }

    (result.status.success(), library, output)
}

// (errors, warnings) per file relative to the code folder
fn problems_per_file(dir: Option<&Path>, diagnostics: &[Diagnostic]) -> std::collections::HashMap<String, (usize, usize)>
{
    let mut problems = std::collections::HashMap::new();
    let Some(dir) = dir.and_then(|dir| fs::canonicalize(dir).ok()) else { return problems; };

    for diagnostic in diagnostics
    {
        let Ok(relative) = diagnostic.file.strip_prefix(&dir) else { continue; };
        let relative = relative.to_string_lossy().replace('\\', "/");

        let entry: &mut (usize, usize) = problems.entry(relative).or_default();
        if diagnostic.error { entry.0 += 1; } else { entry.1 += 1; }
    }

    problems
}

// the primary places of an error or warning - rustc names the files relative to the workspace root (the code folder)
fn parse_diagnostic(dir: &Path, message: &serde_json::Value) -> Vec<Diagnostic>
{
    let level = message["level"].as_str().unwrap_or_default();
    if !level.starts_with("error") && level != "warning"
    {
        return vec![];
    }

    let text = message["message"].as_str().unwrap_or_default().to_string();
    let rendered = message["rendered"].as_str().unwrap_or_default().trim_end().to_string();
    let spans = message["spans"].as_array().cloned().unwrap_or_default();

    spans.iter().filter(|span| span["is_primary"].as_bool() == Some(true)).filter_map(|span|
    {
        let file = PathBuf::from(span["file_name"].as_str()?);
        let file = if file.is_absolute() { file } else { dir.join(file) };
        // canonical: the code editor and the hierarchy compare it with their files
        let file = fs::canonicalize(&file).unwrap_or(file);
        let number = |key: &str| span[key].as_u64().map(|value| value as usize);

        Some(Diagnostic
        {
            file,
            error: level.starts_with("error"),
            message: text.clone(),
            rendered: rendered.clone(),
            line_start: number("line_start")?,
            column_start: number("column_start")?,
            line_end: number("line_end")?,
            column_end: number("column_end")?,
        })
    }).collect()
}

// paths and modification times of the sources - a change starts a build
fn scan_sources(dir: &Path) -> (Vec<String>, u64)
{
    let mut files = vec![];
    collect_files(dir, dir, &mut files);
    files.sort();

    let mut hasher = DefaultHasher::new();
    for file in &files
    {
        file.hash(&mut hasher);
        if let Ok(meta) = fs::metadata(dir.join(file))
        {
            meta.len().hash(&mut hasher);
            meta.modified().ok().hash(&mut hasher);
        }
    }

    (files, hasher.finish())
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<String>)
{
    let Ok(entries) = fs::read_dir(dir) else { return; };

    for entry in entries.flatten()
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') || name == "target" || name == "Cargo.lock"
        {
            continue;
        }

        if path.is_dir()
        {
            collect_files(root, &path, files);
        }
        else if let Ok(relative) = path.strip_prefix(root)
        {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn read_crate_name(cargo_toml: &Path) -> Option<String>
{
    let text = fs::read_to_string(cargo_toml).ok()?;

    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("name").map(str::trim).and_then(|rest| rest.strip_prefix('=')))
        .map(|value| value.trim().trim_matches('"').to_string())
}

pub fn code_dir(project_file: &Path) -> PathBuf
{
    project_file.parent().unwrap_or(Path::new("")).join(CODE_DIR)
}

// ******************** open ********************

// the code folder in its own VS Code window (rust-analyzer finds the project by it) - with a file: that file in it
fn open_vscode(dir: &Path, file: Option<&Path>) -> bool
{
    let mut args = vec![dir.as_os_str().to_os_string()];
    if let Some(file) = file
    {
        args.push("-g".into());
        args.push(file.as_os_str().to_os_string());
    }

    let args: Vec<&std::ffi::OsStr> = args.iter().map(|arg| arg.as_os_str()).collect();
    vscode_command(&args).status().is_ok_and(|status| status.success())
}

pub fn open_in_vscode(dir: &Path)
{
    let dir = dir.to_path_buf();

    std::thread::spawn(move ||
    {
        if !open_vscode(&dir, None)
        {
            console_error!("can not start VS Code (code)");
        }
    });
}

// a source file in the VS Code window of its code folder - without VS Code in the default app
pub fn open_code_file(dir: &Path, file: &Path)
{
    let (dir, file) = (dir.to_path_buf(), file.to_path_buf());

    std::thread::spawn(move ||
    {
        if !open_vscode(&dir, Some(&file))
        {
            open_with_default_app(&file);
        }
    });
}

// windows: code is code.cmd, which only cmd finds
fn vscode_command(args: &[&std::ffi::OsStr]) -> Command
{
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut command = Command::new("cmd");
        command.arg("/C").arg("code").args(args).creation_flags(CREATE_NO_WINDOW);
        command
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut command = Command::new("code");
        command.args(args);
        command
    }
}

// ******************** new project ********************

fn crate_name_for(project_name: &str) -> String
{
    let mut name = String::new();
    for c in project_name.to_lowercase().chars()
    {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        if c == '_' && (name.is_empty() || name.ends_with('_'))
        {
            continue;
        }
        name.push(c);
    }

    let mut name = name.trim_end_matches('_').to_string();

    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit())
    {
        name = format!("project_{}", name);
    }

    if name.starts_with("rustl")
    {
        name = format!("{}_game", name);
    }

    name
}

// the engine for the Cargo.toml of a project: relative from the code folder (the project can move together with the engine), absolute on another drive
// cargo tells path dependencies apart by the resolved path - the editor and the exports use the real spelling of the code folder, so it is always the engine of the editor
pub fn engine_path_from(dir: &Path) -> String
{
    let root = real_path(&engine_root());

    match relative_path(&real_path(dir), &root)
    {
        Some(path) if path.is_empty() => ".".to_string(),
        Some(path) => path,
        None => normalize_path_separators(&root.to_string_lossy()),
    }
}

// the code crate of a new project - wherever the project is
pub fn create_project_code(project_dir: &Path, project_name: &str) -> Result<PathBuf, String>
{
    let dir = project_dir.join(CODE_DIR);

    if dir.join("Cargo.toml").exists()
    {
        return Ok(dir);
    }

    let root = engine_path_from(&real_path(&dir));
    let crate_name = crate_name_for(project_name);

    let files =
    [
        ("Cargo.toml", TEMPLATE_CARGO_TOML),
        ("src/app.rs", TEMPLATE_APP_RS),
        ("src/main.rs", TEMPLATE_MAIN_RS),
        (".cargo/config.toml", TEMPLATE_CARGO_CONFIG),
    ];

    for (file, template) in files
    {
        let path = dir.join(file);
        let content = template.replace("__crate__", &crate_name).replace("__root__", &root);

        fs::create_dir_all(path.parent().unwrap()).map_err(|err| format!("{}: {}", path.display(), err))?;
        fs::write(&path, content).map_err(|err| format!("{}: {}", path.display(), err))?;
    }

    console_success!("project code created: {}", dir.display());
    Ok(dir)
}


// ******************** loading ********************

mod hot
{
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use crate::interface::app::{APP_FACTORY_SYMBOL, AppFactory};

    #[derive(Default)]
    pub struct Loader
    {
        // never unloaded: closures and vtables of an old build may still be referenced by the engine
        libraries: Vec<libloading::Library>,
        loaded: Option<(PathBuf, SystemTime)>,
        factory: Option<AppFactory>,
        copies: u32,
    }

    impl Loader
    {
        pub fn factory(&mut self, library: &Path) -> Result<AppFactory, String>
        {
            let modified = fs::metadata(library).and_then(|meta| meta.modified()).map_err(|err| err.to_string())?;
            let current = Some((library.to_path_buf(), modified));

            if self.loaded != current || self.factory.is_none()
            {
                let copy = self.copy(library)?;

                let handle = unsafe { libloading::Library::new(&copy) }.map_err(|err| err.to_string())?;
                let factory = unsafe { handle.get::<AppFactory>(APP_FACTORY_SYMBOL.as_bytes()) }.map_err(|err| format!("{} not found: {}", APP_FACTORY_SYMBOL, err))?;

                self.factory = Some(*factory);
                self.libraries.push(handle);
                self.loaded = current;
            }

            self.factory.ok_or("no app factory".to_string())
        }

        // windows locks a loaded dll - cargo has to replace the original with the next build
        fn copy(&mut self, library: &Path) -> Result<PathBuf, String>
        {
            let hot_dir = library.parent().unwrap_or(Path::new(".")).join("hot");

            if self.copies == 0
            {
                // leftovers of earlier sessions - the ones still loaded by another editor stay
                let _ = fs::remove_dir_all(&hot_dir);
            }
            fs::create_dir_all(&hot_dir).map_err(|err| err.to_string())?;

            self.copies += 1;
            let stem = library.file_stem().unwrap_or_default().to_string_lossy();
            let extension = library.extension().unwrap_or_default().to_string_lossy();
            let copy = hot_dir.join(format!("{}_{}_{}.{}", stem, std::process::id(), self.copies, extension));

            fs::copy(library, &copy).map_err(|err| format!("copy to {}: {}", copy.display(), err))?;
            Ok(copy)
        }
    }
}
