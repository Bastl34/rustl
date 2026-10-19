// small save data of a game (settings, highscores, progress) as text per key, usually json
// native: <user data folder>/<game>/<key>.json (Windows %APPDATA%, macOS ~/Library/Application Support, else $XDG_DATA_HOME or ~/.local/share)
// web: localStorage "<game>/<key>" - only where a window is (not in a worker)

// the folder of a game's save data, None if the system has no user folder
#[cfg(not(target_arch = "wasm32"))]
pub fn save_folder(game: &str) -> Option<std::path::PathBuf>
{
    use std::path::PathBuf;

    let env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "windows")
    {
        env("APPDATA")
    }
    else if cfg!(target_os = "macos")
    {
        env("HOME").map(|home| home.join("Library").join("Application Support"))
    }
    else
    {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local").join("share")))
    }?;

    Some(base.join(file_name(game)))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_text(game: &str, key: &str, text: &str) -> Result<(), String>
{
    let folder = save_folder(game).ok_or("no user data folder")?;
    std::fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;

    // written next to it first - a crash while writing leaves the old file
    let path = folder.join(format!("{}.json", file_name(key)));
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, text).map_err(|error| format!("{}: {error}", temp.display()))?;
    std::fs::rename(&temp, &path).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_text(game: &str, key: &str) -> Option<String>
{
    let path = save_folder(game)?.join(format!("{}.json", file_name(key)));
    std::fs::read_to_string(path).ok()
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage>
{
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn save_text(game: &str, key: &str, text: &str) -> Result<(), String>
{
    let storage = storage().ok_or("no local storage")?;
    storage.set_item(&format!("{}/{}", file_name(game), file_name(key)), text).map_err(|error| format!("{error:?}"))
}

#[cfg(target_arch = "wasm32")]
pub fn load_text(game: &str, key: &str) -> Option<String>
{
    storage()?.get_item(&format!("{}/{}", file_name(game), file_name(key))).ok()?
}

// a name that works as a file name everywhere
fn file_name(name: &str) -> String
{
    let cleaned: String = name.trim().chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') { c } else { '_' }).collect();
    if cleaned.is_empty() { "game".to_string() } else { cleaned }
}
