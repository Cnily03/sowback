pub mod console;
pub mod formatter;
pub mod logger;
pub mod macros;

// Re-export public items for easy access
pub use formatter::{format_client_info, format_service_config, format_uuid};
pub use logger::{init_logger, LoggerConfig};
// pub use macros::*;
