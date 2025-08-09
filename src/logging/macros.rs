/// Print only in console briefly
#[macro_export]
macro_rules! console_error {
    ($($arg:tt)*) => {
        $crate::logging::console::console_log_non_verbose(
            $crate::logging::console::ConsoleLevel::Error,
            &format!($($arg)*)
        );
    };
}

/// Print only in console briefly
#[macro_export]
macro_rules! console_warn {
    ($($arg:tt)*) => {
        $crate::logging::console::console_log_non_verbose(
            $crate::logging::console::ConsoleLevel::Warn,
            &format!($($arg)*)
        );
    };
}

/// Print only in console briefly
#[macro_export]
macro_rules! console_info {
    ($($arg:tt)*) => {
        $crate::logging::console::console_log_non_verbose(
            $crate::logging::console::ConsoleLevel::Info,
            &format!($($arg)*)
        );
    };
}

/// Print only in console briefly
#[macro_export]
macro_rules! console_debug {
    ($($arg:tt)*) => {
        $crate::logging::console::console_log_non_verbose(
            $crate::logging::console::ConsoleLevel::Debug,
            &format!($($arg)*)
        );
    };
}

/// Print only in console briefly
#[macro_export]
macro_rules! console_trace {
    ($($arg:tt)*) => {
        $crate::logging::console::console_log_non_verbose(
            $crate::logging::console::ConsoleLevel::Trace,
            &format!($($arg)*)
        );
    };
}

/// Detail logging
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!($($arg)*);
    };
}

/// Detail logging
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!($($arg)*);
    };
}

/// Detail logging
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*);
    };
}

/// Detail logging
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*);
    };
}

/// Detail logging
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        tracing::trace!($($arg)*);
    };
}

/// both `console_` and `log_`
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log_error!($($arg)*);
        $crate::console_error!($($arg)*);
    };
}

/// both `console_` and `log_`
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log_warn!($($arg)*);
        $crate::console_warn!($($arg)*);
    };
}

/// both `console_` and `log_`
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log_info!($($arg)*);
        $crate::console_info!($($arg)*);
    };
}

/// both `console_` and `log_`
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log_debug!($($arg)*);
        $crate::console_debug!($($arg)*);
    };
}

/// both `console_` and `log_`
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::log_trace!($($arg)*);
        $crate::console_trace!($($arg)*);
    };
}
