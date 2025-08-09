use chrono::Local;
use colored::{ColoredString, Colorize};
use std::io::{self, Write};

use crate::logging::logger::LoggerConfig;

/// Format current time as H:M:S string
pub fn format_local_time() -> String {
    Local::now().format("%H:%M:%S").to_string()
}

/// Check if the terminal supports color output
pub fn supports_color() -> bool {
    // Check various environment variables and terminal capabilities
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    if std::env::var("FORCE_COLOR").is_ok() {
        return true;
    }

    // Check if we're in a TTY
    if !atty::is(atty::Stream::Stdout) && !atty::is(atty::Stream::Stderr) {
        return false;
    }

    // Check TERM environment variable
    if let Ok(term) = std::env::var("TERM") {
        if term == "dumb" {
            return false;
        }
        // Most modern terminals support color
        return !term.is_empty();
    }

    // Default to supporting color on Unix-like systems
    cfg!(unix)
}

/// Console log levels ordered by severity (most severe first)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsoleLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl ConsoleLevel {
    /// Get the colored string representation of the log level (right-aligned)
    pub fn as_colored_str(&self) -> ColoredString {
        match self {
            ConsoleLevel::Error => format!("{:>5}", "ERROR").red(),
            ConsoleLevel::Warn => format!("{:>5}", "WARN").yellow(),
            ConsoleLevel::Info => format!("{:>5}", "INFO").green(),
            ConsoleLevel::Debug => format!("{:>5}", "DEBUG").cyan(),
            ConsoleLevel::Trace => format!("{:>5}", "TRACE").magenta(),
        }
    }

    /// Get the plain string representation of the log level (right-aligned)
    pub fn as_str(&self) -> String {
        match self {
            ConsoleLevel::Error => format!("{:>5}", "ERROR"),
            ConsoleLevel::Warn => format!("{:>5}", "WARN"),
            ConsoleLevel::Info => format!("{:>5}", "INFO"),
            ConsoleLevel::Debug => format!("{:>5}", "DEBUG"),
            ConsoleLevel::Trace => format!("{:>5}", "TRACE"),
        }
    }

    /// Get the appropriate string representation based on terminal color support
    pub fn as_display_str(&self) -> String {
        if supports_color() {
            self.as_colored_str().to_string()
        } else {
            self.as_str()
        }
    }
}

/// Format and print a console message
pub fn console_log(level: ConsoleLevel, message: &str) {
    let time_str = if supports_color() {
        format_local_time().dimmed().to_string()
    } else {
        format_local_time()
    };

    // Use appropriate level string based on color support
    let level_str = level.as_display_str();

    let formatted = format!("{} {} {}", time_str, level_str, message);

    match level {
        ConsoleLevel::Error | ConsoleLevel::Warn | ConsoleLevel::Trace => {
            let _ = writeln!(io::stderr(), "{}", formatted);
            let _ = io::stderr().flush();
        }
        ConsoleLevel::Info | ConsoleLevel::Debug => {
            let _ = writeln!(io::stdout(), "{}", formatted);
            let _ = io::stdout().flush();
        }
    }
}

pub fn console_log_non_verbose(level: ConsoleLevel, message: &str) {
    if !LoggerConfig::get_global_clone().verbose {
        console_log(level, message);
    }
}
