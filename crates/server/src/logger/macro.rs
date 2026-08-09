#[macro_export]
macro_rules! info {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = crate::logger::Entry {
                module: $module,
                level: crate::logger::Level::Info,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

#[macro_export]
macro_rules! warn {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = crate::logger::Entry {
                module: $module,
                level: crate::logger::Level::Warning,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

#[macro_export]
macro_rules! error {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = crate::logger::Entry {
                module: $module,
                level: crate::logger::Level::Error,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

#[macro_export]
macro_rules! shutdown {
    ($($arg:tt)+) => {
        if let Some(logger) = crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = crate::logger::Entry {
                module: crate::logger::Module::Server,
                level: crate::logger::Level::Shutdown,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
	};
}

#[macro_export]
macro_rules! fatal {
    ($module:expr, $($arg:tt)+) => {
        if let Some(logger) = crate::logger::GLOBAL_LOGGER.get() {
            let message = format!($($arg)+);

            let entry = crate::logger::Entry {
                module: $module,
                level: crate::logger::Level::Fatal,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                message,
            };
            let _ = logger.try_send(entry);
        }
    }
}
