use std::sync::{Mutex, OnceLock};

/// Global logging configuration
static LOGGER_CONFIG: OnceLock<Mutex<LoggerConfig>> = OnceLock::new();

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    pub log_file: Option<String>,
    pub verbose: bool,
}

impl LoggerConfig {
    pub fn get_global_clone() -> LoggerConfig {
        LOGGER_CONFIG.get().unwrap().lock().unwrap().clone()
    }
}

/// Initialize the logging system
pub fn init_logger(log_file: Option<String>, verbose: bool) {
    let config = LoggerConfig {
        log_file: log_file.clone(),
        verbose,
    };
    LOGGER_CONFIG.set(Mutex::new(config.clone())).unwrap();
    // Initialize tracing subscriber with the provided configuration
    init_tracing(&config);
}

/// Initialize tracing subscriber with different modes
pub fn init_tracing(config: &LoggerConfig) {
    use tracing_subscriber::{
        fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
    };

    let env_filter_base = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let env_filter = EnvFilter::new(&env_filter_base);

    // console detail layer (if verbose enabled)
    let console_detail_layer = if config.verbose {
        let layer = fmt::Layer::new()
            .with_target(true)
            .with_level(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_ansi(true);

        Some(layer)
    } else {
        None
    };

    // file JSON layer (if file specified)
    let file_json_layer = if let Some(log_file_path) = &config.log_file {
        let file_appender = tracing_appender::rolling::never(".", log_file_path);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        let layer = fmt::Layer::new()
            .with_writer(non_blocking)
            .with_target(true)
            .with_level(true)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_ansi(false)
            .json();

        std::mem::forget(_guard);

        Some(layer)
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_detail_layer)
        .with(file_json_layer)
        .init();
}
