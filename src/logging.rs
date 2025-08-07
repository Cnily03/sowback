use colored::*;
use std::sync::{Mutex, OnceLock};
use std::fmt;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::field::Visit;

/// Global logging configuration
static LOGGER_CONFIG: OnceLock<Mutex<LoggerConfig>> = OnceLock::new();

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    pub log_file: Option<String>,
    pub verbose: bool,
}

/// Log modes using bitflags
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogMode {
    ConsoleBrief = 1 << 0,
    ConsoleDetail = 1 << 1,
    FileJson = 1 << 2,
}

impl LogMode {
    /// Check if the given mode flags contain this mode
    pub fn matches(self, flags: u8) -> bool {
        (flags & (self as u8)) != 0
    }
}

/// A conditional layer that only logs events with matching mode
pub struct ModeLayer<L> {
    inner: L,
    target_mode: LogMode,
}

impl<L> ModeLayer<L> {
    pub fn new(inner: L, target_mode: LogMode) -> Self {
        Self { inner, target_mode }
    }
}

impl<S, L> tracing_subscriber::layer::Layer<S> for ModeLayer<L>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
    L: tracing_subscriber::layer::Layer<S>,
{
    /// If mode matches, continue the event to be logged to this layer.
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Extract mode value from event
        struct ModeVisitor {
            mode_value: Option<u8>,
        }
        
        impl Visit for ModeVisitor {
            fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
                if field.name() == "mode" {
                    self.mode_value = Some(value as u8);
                }
            }
            
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "mode" {
                    if let Ok(val) = format!("{value:?}").parse::<u8>() {
                        self.mode_value = Some(val);
                    }
                }
            }
        }
        
        let mut visitor = ModeVisitor { mode_value: None };
        event.record(&mut visitor);

        // Check if this event should be logged to this layer
        if let Some(mode_value) = visitor.mode_value {
            if self.target_mode.matches(mode_value) {
                self.inner.on_event(event, ctx);
            }
        } else {
            // No mode field, always log
            self.inner.on_event(event, ctx);
        }
    }

    fn on_new_span(&self, attrs: &tracing::span::Attributes<'_>, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_new_span(attrs, id, ctx);
    }

    fn on_record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_record(id, values, ctx);
    }

    fn on_follows_from(&self, id: &tracing::span::Id, follows: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_follows_from(id, follows, ctx);
    }

    fn on_enter(&self, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_enter(id, ctx);
    }

    fn on_exit(&self, id: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_exit(id, ctx);
    }

    fn on_close(&self, id: tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_close(id, ctx);
    }

    fn on_id_change(&self, old: &tracing::span::Id, new: &tracing::span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        self.inner.on_id_change(old, new, ctx);
    }
}

/// Custom time formatter that shows only hours:minutes:seconds in local time
struct LocalTimeShort;

impl FormatTime for LocalTimeShort {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> fmt::Result {
        use chrono::Local;
        let now = Local::now();
        write!(w, "{}", now.format("%H:%M:%S"))
    }
}

/// Initialize the logging system
pub fn init_logger(log_file: Option<String>, verbose: bool) {
    let config = LoggerConfig { log_file: log_file.clone(), verbose };
    LOGGER_CONFIG.set(Mutex::new(config.clone())).unwrap();
    // Initialize tracing subscriber with the provided configuration
    init_tracing(&config);
}


/// Initialize tracing subscriber with different modes
pub fn init_tracing(config: &LoggerConfig) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, Layer};
    
    let env_filter_base = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    
    // Create console brief layer with conditional logging
    let console_brief_layer = ModeLayer::new(
        fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_ansi(true)
            .with_timer(LocalTimeShort),
        LogMode::ConsoleBrief
    ).boxed();
    
    // Create console detail layer with conditional logging
    let console_detail_layer = ModeLayer::new(
        fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_ansi(true),
        LogMode::ConsoleDetail
    ).boxed();
    
    // Create optional file JSON layer
    let file_json_layer = if let Some(log_file_path) = &config.log_file {
        let file_appender = tracing_appender::rolling::never(".", log_file_path);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        let layer = ModeLayer::new(
            fmt::layer()
                .with_writer(non_blocking)
                .with_target(true)
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_ansi(false)
                .json(),
            LogMode::FileJson
        ).boxed();
        
        // Keep the guard alive for the duration of the program
        std::mem::forget(_guard);
        Some(layer)
    } else {
        None
    };
    
    // Combine all layers
    let registry = tracing_subscriber::registry()
        .with(console_brief_layer)
        .with(console_detail_layer);
    
    if let Some(file_layer) = file_json_layer {
        registry.with(file_layer).init();
    } else {
        registry.init();
    }
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

#[macro_export]
macro_rules! mode_log {
    () => {
        {
            let config = $crate::logging::get_logger_config();
            if config.verbose {
                $crate::logging::LogMode::FileJson as u8 | $crate::logging::LogMode::ConsoleDetail as u8
            } else {
                $crate::logging::LogMode::FileJson as u8
            }
        }
    };
}

#[macro_export]
macro_rules! mode_console {
    () => {
        {
            let config = $crate::logging::get_logger_config();
            if config.verbose {
                0
            } else {
                $crate::logging::LogMode::ConsoleBrief as u8
            }
        }
    };
}

#[macro_export]
macro_rules! mode_multi {
    () => {
        {
            let config = $crate::logging::get_logger_config();
            if config.verbose {
                $crate::logging::LogMode::FileJson as u8 | $crate::logging::LogMode::ConsoleDetail as u8
            } else {
                $crate::logging::LogMode::FileJson as u8 | $crate::logging::LogMode::ConsoleBrief as u8
            }
        }
    };
}

/// Detail logging.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!(mode = $crate::mode_log!(), $($arg)*);
    };
}

/// Detail logging.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!(mode = $crate::mode_log!(), $($arg)*);
    };
}

/// Detail logging.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!(mode = $crate::mode_log!(), $($arg)*);
    };
}

/// Detail logging.
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!(mode = $crate::mode_log!(), $($arg)*);
    };
}


/// Only brief logging. Use `log_info` to record and trace.
#[macro_export]
macro_rules! console_info {
    ($($arg:tt)*) => {
        tracing::info!(mode = $crate::mode_console!(), $($arg)*);
    };
}

/// Only brief logging. Use `log_warn` to record and trace.
#[macro_export]
macro_rules! console_warn {
    ($($arg:tt)*) => {
        tracing::warn!(mode = $crate::mode_console!(), $($arg)*);
    };
}

/// Only brief logging. Use `log_error` to record and trace.
#[macro_export]
macro_rules! console_error {
    ($($arg:tt)*) => {
        tracing::error!(mode = $crate::mode_console!(), $($arg)*);
    };
}

/// Only brief logging. Use `log_debug` to record and trace.
#[macro_export]
macro_rules! console_debug {
    ($($arg:tt)*) => {
        tracing::debug!(mode = $crate::mode_console!(), $($arg)*);
    };
}



/// Multi brief console logging and detail logging.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        tracing::info!(mode = $crate::mode_multi!(), $($arg)*);
    };
}

/// Multi brief console logging and detail logging.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        tracing::warn!(mode = $crate::mode_multi!(), $($arg)*);
    };
}

/// Multi brief console logging and detail logging.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        tracing::error!(mode = $crate::mode_multi!(), $($arg)*);
    };
}

/// Multi brief console logging and detail logging.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        tracing::debug!(mode = $crate::mode_multi!(), $($arg)*);
    };
}