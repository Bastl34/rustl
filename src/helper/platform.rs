use std::env;

pub fn is_windows() -> bool
{
    env::consts::OS == "windows"
}

pub fn is_linux() -> bool
{
    env::consts::OS == "linux"
}

pub fn is_mac() -> bool
{
    env::consts::OS == "macos"
}

pub fn is_ios() -> bool
{
    env::consts::OS == "ios"
}
pub fn is_android() -> bool
{
    env::consts::OS == "android"
}

pub fn is_web() -> bool
{
    cfg!(target_arch = "wasm32")
}