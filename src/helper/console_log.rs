#![allow(dead_code)]

use chrono::{DateTime, Local};
#[cfg(not(target_arch = "wasm32"))]
use colored::*;
use std::{sync::{LazyLock, Mutex}};

const MAX_LOGS: usize = 10_000;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LogType
{
    All, // <- just used as identfier
    Log,
    Warning,
    Success,
    Error,
    Debug
}

// where a log comes from - the console filters by it
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LogSource
{
    Engine,
    // the code of the project: its compiler output and its own logs
    Code,
}

#[derive(Clone)]
pub struct LogEntry
{
    pub timestamp: DateTime<Local>,
    pub log_type: LogType,
    pub source: LogSource,
    pub log: String,
}

pub struct Logs
{
    pub max_logs: usize,
    pub logs: Vec<LogEntry>,
}

impl Default for Logs
{
    fn default() -> Self
    {
        Logs
        {
            max_logs: MAX_LOGS,
            logs: Vec::new()
        }
    }
}

static CONSOLE: LazyLock<Mutex<Logs>> = LazyLock::new(|| Mutex::new(Logs::default()));


#[macro_export]
macro_rules! log_base
{
    // with format
    ($log_type:expr, $fmt:expr, $($arg:tt)*) =>
    {
        {
            let msg = format!($fmt, $($arg)*);
            $crate::helper::console_log::log_from_module(&msg, $log_type, module_path!());
        }
    };
    // without format
    ($log_type:expr, $($arg:expr),+) =>
    {
        {
            let mut msg = vec![$(format!("{:?}", $arg)),+].join(" ");
            msg = msg.strip_prefix('"').unwrap_or(&msg).to_string();
            msg = msg.strip_suffix('"').unwrap_or(&msg).to_string();

            $crate::helper::console_log::log_from_module(&msg, $log_type, module_path!());
        }
    };
    // single argument
    ($log_type:expr, $arg:expr) =>
    {
        {
            let mut msg = format!("{:?}", $arg);
            msg = msg.strip_prefix('"').unwrap_or(&msg).to_string();
            msg = msg.strip_suffix('"').unwrap_or(&msg).to_string();

            $crate::helper::console_log::log_from_module(&msg, $log_type, module_path!());
        }
    };
}

#[macro_export]
macro_rules! console_log
{
    ($fmt:expr, $($arg:tt)*) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Log,
            $fmt, $($arg)*
        );
    };
    ($($arg:expr),+) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Log,
            $($arg)+
        );
    };
    ($arg:expr) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Log,
            $arg
        );
    };
}

#[macro_export]
macro_rules! console_error
{
    ($fmt:expr, $($arg:tt)*) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Error,
            $fmt, $($arg)*
        );
    };
    ($($arg:expr),+) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Error,
            $($arg)+
        );
    };
    ($arg:expr) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Error,
            $arg
        );
    };
}

#[macro_export]
macro_rules! console_success
{
    ($fmt:expr, $($arg:tt)*) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Success,
            $fmt, $($arg)*
        );
    };
    ($($arg:expr),+) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Success,
            $($arg)+
        );
    };
    ($arg:expr) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Success,
            $arg
        );
    };
}

#[macro_export]
macro_rules! console_warning
{
    ($fmt:expr, $($arg:tt)*) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Warning,
            $fmt, $($arg)*
        );
    };
    ($($arg:expr),+) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Warning,
            $($arg)+
        );
    };
    ($arg:expr) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Warning,
            $arg
        );
    };
}

#[macro_export]
macro_rules! console_debug
{
    ($fmt:expr, $($arg:tt)*) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Debug,
            $fmt, $($arg)*
        );
    };
    ($($arg:expr),+) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Debug,
            $($arg)+
        );
    };
    ($arg:expr) =>
    {
        $crate::log_base!
        (
            $crate::helper::console_log::LogType::Debug,
            $arg
        );
    };
}

pub fn get_mutex() -> &'static LazyLock<Mutex<Logs>>
{
    &CONSOLE
}

pub fn get_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.len()
}

pub fn get_log_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.log_type == LogType::Log).count()
}

pub fn get_error_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.log_type == LogType::Error).count()
}

pub fn get_warnings_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.log_type == LogType::Warning).count()
}

pub fn get_success_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.log_type == LogType::Success).count()
}

pub fn get_debug_amount() -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.log_type == LogType::Debug).count()
}

pub fn get_source_amount(source: LogSource) -> usize
{
    CONSOLE.lock().unwrap().logs.iter().filter(|log| log.source == source).count()
}

pub fn log(msg: &str, log_type: LogType)
{
    log_from(msg, log_type, LogSource::Engine);
}

// the console_*! macros - module_path of the caller: rustl::... is the engine, everything else the code of a project
pub fn log_from_module(msg: &str, log_type: LogType, module: &str)
{
    let source = if module.starts_with("rustl") { LogSource::Engine } else { LogSource::Code };
    log_from(msg, log_type, source);
}

pub fn log_from(msg: &str, log_type: LogType, source: LogSource)
{
    // only the editor console reads it - and the web main thread must not wait for a lock a worker holds (Atomics.wait panics there)
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut logs = CONSOLE.lock().unwrap();
        logs.logs.push
        (
            LogEntry
            {
                timestamp: Local::now(),
                log_type: log_type.clone(),
                source,
                log: msg.to_string(),
            }
        );

        if logs.logs.len() > logs.max_logs
        {
            logs.logs.remove(0);
        }
    }

    // println goes nowhere on the web
    #[cfg(target_arch = "wasm32")]
    {
        let _ = source;
        let msg = wasm_bindgen::JsValue::from_str(msg);
        match log_type
        {
            LogType::Error => web_sys::console::error_1(&msg),
            LogType::Warning => web_sys::console::warn_1(&msg),
            LogType::Debug => web_sys::console::debug_1(&msg),
            _ => web_sys::console::log_1(&msg),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    match log_type
    {
        LogType::All => println!("{}", msg),
        LogType::Log => println!("{}", msg),
        LogType::Error => println!("{}", msg.red()),
        LogType::Success => println!("{}", msg.green()),
        LogType::Warning => println!("{}", msg.yellow()),
        LogType::Debug => println!("{}", msg.bright_blue()),
    }
}