use colored::*;
use serde_json::json;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use chrono::{DateTime, Local};

/// Global logging configuration
static LOGGER_CONFIG: OnceLock<Mutex<LoggerConfig>> = OnceLock::new();

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    pub log_file: Option<String>,
    pub verbose: bool,
}

/// Log levels
#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

/// Initialize the logging system
pub fn init_logger(log_file: Option<String>, verbose: bool) {
    let config = LoggerConfig { log_file, verbose };
    LOGGER_CONFIG.set(Mutex::new(config)).unwrap();
}

/// Get current logger configuration
pub fn get_logger_config() -> LoggerConfig {
    LOGGER_CONFIG.get().unwrap().lock().unwrap().clone()
}

/// Formats a UUID for display with color coding based on its purpose
pub fn format_uuid(uuid: &str, purpose: &str) -> String {
    let short_uuid = &uuid[..8];
    match purpose {
        "conn" => short_uuid.yellow().to_string(),
        "proxy" => short_uuid.green().to_string(),
        "client" | "server" => short_uuid.blue().to_string(),
        _ => short_uuid.normal().to_string(),
    }
}

/// Formats client identification information with optional name and IP address
pub fn format_client_info(name: Option<&str>, addr: &str) -> String {
    match name {
        Some(n) if !n.is_empty() => format!("{} ({})", n.cyan(), addr),
        _ => addr.to_string(),
    }
}

/// Formats service configuration for display with color coding
pub fn format_service_config(local_ip: &str, local_port: u16, remote_port: u16) -> String {
    format!("{}:{} -> :{}", local_ip.magenta(), local_port.to_string().magenta(), remote_port.to_string().green())
}

/// Internal logging function that handles both verbose console output and file logging
pub fn log_message(level: LogLevel, message: &str, details: Option<serde_json::Value>) {
    let config = get_logger_config();
    let now: DateTime<Local> = Local::now();
    
    // Verbose console output
    if config.verbose {
        let timestamp = now.format("%Y-%m-%d %H:%M:%S");
        let level_str = match level {
            LogLevel::Info => "[INFO]".green().bold(),
            LogLevel::Warn => "[WARN]".yellow().bold(),
            LogLevel::Error => "[ERROR]".red().bold(),
            LogLevel::Debug => "[DEBUG]".blue().bold(),
        };
        
        if let Some(details) = &details {
            println!("{} {} {} {}", timestamp.to_string().cyan(), level_str, message, 
                     format!("details={}", details).dimmed());
        } else {
            println!("{} {} {}", timestamp.to_string().cyan(), level_str, message);
        }
    }
    
    // File output (JSON format)
    if let Some(log_file) = &config.log_file {
        let level_str = match level {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN", 
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        };
        
        let mut log_entry = json!({
            "timestamp": now.to_rfc3339(),
            "level": level_str,
            "message": message
        });
        
        if let Some(details) = details {
            log_entry["details"] = details;
        }
        
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file) {
            let _ = writeln!(file, "{}", log_entry);
        }
    }
}

/// Console output with custom format (for non-verbose mode)
pub fn console_message(level: LogLevel, message: &str) {
    let config = get_logger_config();
    
    // Only output to console if not in verbose mode (verbose mode uses log_message)
    if !config.verbose {
        let now: DateTime<Local> = Local::now();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S");
        let level_str = match level {
            LogLevel::Info => "[INFO]".green().bold(),
            LogLevel::Warn => "[WARN]".yellow().bold(),
            LogLevel::Error => "[ERROR]".red().bold(),
            LogLevel::Debug => "[DEBUG]".blue().bold(),
        };
        
        println!("{} {} {}", timestamp.to_string().cyan(), level_str, message);
    }
}

/// Macro for detailed logging (verbose console + file)
#[macro_export]
macro_rules! log_info {
    ($msg:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Info, $msg, None);
    };
    ($msg:expr, $details:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Info, $msg, Some($details));
    };
}

#[macro_export]
macro_rules! log_warn {
    ($msg:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Warn, $msg, None);
    };
    ($msg:expr, $details:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Warn, $msg, Some($details));
    };
}

#[macro_export]
macro_rules! log_error {
    ($msg:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Error, $msg, None);
    };
    ($msg:expr, $details:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Error, $msg, Some($details));
    };
}

#[macro_export]
macro_rules! log_debug {
    ($msg:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Debug, $msg, None);
    };
    ($msg:expr, $details:expr) => {
        $crate::logging::log_message($crate::logging::LogLevel::Debug, $msg, Some($details));
    };
}

/// Macro for frontend output (calls log_info! + custom console output in non-verbose mode)
#[macro_export]
macro_rules! info {
    ($msg:expr) => {
        $crate::log_info!($msg);
        $crate::logging::console_message($crate::logging::LogLevel::Info, $msg);
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::log_info!(msg.as_str());
        $crate::logging::console_message($crate::logging::LogLevel::Info, &msg);
    };
}

#[macro_export]
macro_rules! warn {
    ($msg:expr) => {
        $crate::log_warn!($msg);
        $crate::logging::console_message($crate::logging::LogLevel::Warn, $msg);
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::log_warn!(msg.as_str());
        $crate::logging::console_message($crate::logging::LogLevel::Warn, &msg);
    };
}

#[macro_export]
macro_rules! error {
    ($msg:expr) => {
        $crate::log_error!($msg);
        $crate::logging::console_message($crate::logging::LogLevel::Error, $msg);
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::log_error!(msg.as_str());
        $crate::logging::console_message($crate::logging::LogLevel::Error, &msg);
    };
}

#[macro_export]
macro_rules! debug {
    ($msg:expr) => {
        $crate::log_debug!($msg);
        $crate::logging::console_message($crate::logging::LogLevel::Debug, $msg);
    };
    ($fmt:expr, $($arg:tt)*) => {
        let msg = format!($fmt, $($arg)*);
        $crate::log_debug!(msg.as_str());
        $crate::logging::console_message($crate::logging::LogLevel::Debug, &msg);
    };
}
