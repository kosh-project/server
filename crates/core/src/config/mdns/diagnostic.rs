//! Rich compiler-grade diagnostic errors for mDNS configuration parsing and validation.
//!
//! When a user makes a syntax or semantic error in `config.kdl` under the `mdns` block,
//! these diagnostics integrate with [`miette`] to produce informative terminal error
//! messages with source snippets, colored underlines, and actionable hints.

use miette::{NamedSource, SourceSpan};
use std::sync::Arc;

/// Rich diagnostic errors emitted during mDNS configuration validation.
///
/// Each variant represents a distinct protocol violation or boundary overflow,
/// carrying the source file context and exact byte span within `config.kdl`
/// so that `miette` can format a clear visual indicator.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Diagnostic {
    /// The mDNS hostname fails RFC 1034 / RFC 1123 DNS label rules.
    #[error("Invalid mDNS {{ \x1b[33mhostname\x1b[0m }}: {reason}")]
    #[diagnostic(code(kosh::config::mdns::invalid_hostname), help("{help}"))]
    InvalidHostname {
        /// The offending hostname.
        hostname: String,
        /// Description of the violation.
        reason: &'static str,
        /// Actionable suggestion.
        help: String,
        /// The source file context for terminal rendering.
        #[source_code]
        src: Arc<NamedSource>,
        /// Span of the mDNS configuration node.
        #[label("{reason}")]
        span: SourceSpan,
    },

    /// A declared network interface has an invalid name format.
    #[error("Invalid network interface: {reason}")]
    #[diagnostic(
        code(kosh::config::mdns::invalid_interface),
        help(
            "Specify valid system network interface names (e.g. \"eth0\", \"wlan0\")."
        )
    )]
    InvalidInterface {
        /// The offending interface name.
        interface: String,
        /// Description of the violation.
        reason: &'static str,
        /// The source file context for terminal rendering.
        #[source_code]
        src: Arc<NamedSource>,
        /// Span of the mDNS configuration node.
        #[label("{reason}")]
        span: SourceSpan,
    },

    /// A TXT record property key fails RFC 6763 requirements.
    #[error("Invalid mDNS TXT property key: {reason}")]
    #[diagnostic(
        code(kosh::config::mdns::invalid_property_key),
        help("{help}")
    )]
    InvalidPropertyKey {
        /// The offending property key.
        key: String,
        /// Description of the violation.
        reason: &'static str,
        /// Actionable suggestion.
        help: String,
        /// The source file context for terminal rendering.
        #[source_code]
        src: Arc<NamedSource>,
        /// Span of the offending property node.
        #[label("{reason}")]
        span: SourceSpan,
    },

    /// A single TXT record property value exceeds 128 bytes.
    #[error("mDNS TXT property value exceeds 128-character limit")]
    #[diagnostic(
        code(kosh::config::mdns::value_too_long),
        help(
            "Keep property values concise to fit within standard mDNS response packets. Shorten value for \x1b[33m`{key}`\x1b[0m."
        )
    )]
    ValueTooLong {
        /// The property key associated with this oversized value.
        key: String,
        /// Actual byte length of the value.
        actual_len: usize,
        /// The source file context for terminal rendering.
        #[source_code]
        src: Arc<NamedSource>,
        /// Span of the offending property node.
        #[label(
            "\x1b[0mvalue is \x1b[31m{actual_len} bytes\x1b[0m (max allowed is \x1b[33m128 bytes\x1b[0m)"
        )]
        span: SourceSpan,
    },

    /// The combined payload size of all custom TXT properties exceeds 400 bytes.
    #[error("mDNS TXT records payload exceeds 400-byte safe limit")]
    #[diagnostic(
        code(kosh::config::mdns::payload_too_large),
        help(
            "Total custom \x1b[33mTXT record\x1b[0m size is \x1b[30m{total_len}\x1b[0m bytes. \n\tSafe MTU limit is \x1b[33m400 bytes\x1b[0m to avoid UDP fragmentation over Wi-Fi. Remove or shorten properties."
        )
    )]
    PayloadTooLarge {
        /// Total byte length of all custom TXT properties.
        total_len: usize,
        /// The source file context for terminal rendering.
        #[source_code]
        src: Arc<NamedSource>,
        /// Span of the `txt` child block.
        #[label(
            "total payload size is \x1b[30m{total_len} bytes\x1b[0m (but expected limit is \x1b[33m400 bytes\x1b[0m)"
        )]
        span: SourceSpan,
    },
}

/// A type alias for `Result<T, Diagnostic>`.
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, Diagnostic>;
