use std::{fmt::Display};

/// A lightweight, zero-dependency macro for printing structured debug output to stdout.
///
/// This macro is a no-op in release builds (`#[cfg(debug_assertions)]` guards every call).
/// Output is formatted with a right-aligned "worker" label followed by the message.
///
/// # Examples
///
/// ```rust
/// use webdav_server::log;
///
/// // Simple message with no label
/// log!("Server started");
///
/// // Labeled message (label is right-aligned to 12 characters)
/// log!("SERVER", "Listening on port 6969");
/// log!("STORAGE", format!("committed: {}", "some_file.bin"));
/// ```
#[macro_export]
macro_rules! log {
    ($msg:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            println!("{:>12}    {}", "", $msg);
        }
    }};

    ($worker:expr, $msg:expr $(,)?) => {{
        #[cfg(debug_assertions)]
        {
            println!(
                "\x1b[1m\x1b[34m{:>12}\x1b[0m -> {}",
                $worker, $msg
            );
        }
    }};
}

/// Extension trait that adds ANSI color-formatting methods to any `Display` type.
///
/// This is intended for use inside `log!` calls to highlight important parts of
/// a message. All methods return a new `String` with the appropriate ANSI escape
/// codes applied.
///
/// # Examples
///
/// ```rust
/// use webdav_server::log::Color;
///
/// let msg = "something went wrong".error();
/// let user = "user_42".bold();
/// ```
pub trait Color {
    /// Wraps the value in bold ANSI escape codes.
    fn bold(&self) -> String;
    /// Wraps the value in italic (dim) ANSI escape codes.
    fn italic(&self) -> String;
    /// Wraps the value in yellow ANSI escape codes, used for warnings.
    fn warn(&self) -> String;
    /// Wraps the value in red ANSI escape codes, used for errors.
    fn error(&self) -> String;
    /// Wraps the value in grey ANSI escape codes, used for low-priority debug output.
    fn debug(&self) -> String;
    /// Wraps the value in cyan ANSI escape codes, used for informational messages.
    fn info(&self) -> String;
}

impl<T> Color for T
where
    T: Display,
{
    fn bold(&self) -> String {
        format!("\x1b[1m{self}\x1b[0m")
    }

    fn debug(&self) -> String {
        format!("\x1b[90m{self}\x1b[0m")
    }

    fn error(&self) -> String {
        format!("\x1b[31m{self}\x1b[0m")
    }

    fn info(&self) -> String {
        format!("\x1b[36m{self}\x1b[0m")
    }

    fn italic(&self) -> String {
        format!("\x1b[2m{self}\x1b[0m")
    }

    fn warn(&self) -> String {
        format!("\x1b[33m{self}\x1b[0m")
    }
}
