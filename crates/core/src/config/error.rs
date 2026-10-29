use crate::config::mdns;

/// Errors that can occur while parsing or loading the server configuration.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Error {
    /// The KDL configuration file could not be parsed.
    ///
    /// This variant wraps the rich diagnostic error from the `knuffel` crate, which
    /// includes source location information (line and column) for the offending token.
    /// When displayed via `miette`, this produces a human-friendly error report with
    /// source context highlighted in the terminal.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Parse(#[from] knuffel::Error),

    /// A filesystem I/O error occurred while reading or writing the configuration file.
    #[error("File system I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An mDNS zero-configuration discovery validation rule failed.
    ///
    /// This variant wraps rich diagnostics from [`mdns::Diagnostic`], covering
    /// hostname label length, invalid characters, interface name limits,
    /// or TXT record payload size boundaries according to RFC 6762 and RFC 6763.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Mdns(#[from] mdns::Diagnostic),
}

/// A type alias for `Result<T, config::Error>`.
///
/// The second generic parameter defaults to [`Error`], which allows this alias to be
/// used as a single-argument `Result<T>` throughout the `config` module while remaining
/// compatible with derive macros such as `knuffel::Decode` that need to parameterise the
/// error type independently.
pub type Result<T, E = Error> = core::result::Result<T, E>;
