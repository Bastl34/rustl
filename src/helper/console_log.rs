#![allow(dead_code)]

use chrono::{DateTime, Local};
use colored::*;
use std::{sync::{LazyLock, Mutex}};

const MAX_LOGS: usize = 10_000;

#[derive(Clone, PartialEq)]
pub enum LogType
{
    All, // <- just used as identfier
    Log,
    Warning,
    Success,
    Error
}

#[derive(Clone)]
pub struct LogEntry
{
    pub timestamp: DateTime<Local>,
    pub log_type: LogType,
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
            $crate::helper::console_log::log(&msg, $log_type);
        }
    };
    // without format
    ($log_type:expr, $($arg:expr),+) =>
    {
        {
            let mut msg = vec![$(format!("{:?}", $arg)),+].join(" ");
            msg = msg.strip_prefix('"').unwrap_or(&msg).to_string();
            msg = msg.strip_suffix('"').unwrap_or(&msg).to_string();

            $crate::helper::console_log::log(&msg, $log_type);
        }
    };
    // single argument
    ($log_type:expr, $arg:expr) =>
    {
        {
            let mut msg = format!("{:?}", $arg);
            msg = msg.strip_prefix('"').unwrap_or(&msg).to_string();
            msg = msg.strip_suffix('"').unwrap_or(&msg).to_string();

            $crate::helper::console_log::log(&msg, $log_type);
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


pub fn log(msg: &str, log_type: LogType)
{
    let mut logs = CONSOLE.lock().unwrap();
    logs.logs.push
    (
        LogEntry
        {
            timestamp: Local::now(),
            log_type: log_type.clone(),
            log: msg.to_string(),
        }
    );

    if logs.logs.len() > logs.max_logs
    {
        logs.logs.remove(0);
    }

    match log_type
    {
        LogType::All => println!("{}", msg),
        LogType::Log => println!("{}", msg),
        LogType::Error => println!("{}", msg.red()),
        LogType::Success => println!("{}", msg.green()),
        LogType::Warning => println!("{}", msg.yellow()),
    }
}