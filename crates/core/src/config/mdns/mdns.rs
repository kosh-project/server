//! Top-level mDNS configuration schema and semantic validation logic.

use knuffel::{Decode, span::Span};
use miette::NamedSource;
use std::sync::Arc;

use super::diagnostic::Diagnostic;
use super::txt::Txt;

/// Configuration settings for Multicast DNS (mDNS) and DNS-SD zero-configuration discovery.
///
/// In local networks, mDNS allows clients to discover the Kosh server by querying for
/// `_http._tcp.local.` or `_https._tcp.local.` service pointers or directly resolving
/// `<hostname>.local` without requiring a centralized DNS server.
///
/// # Example KDL configuration
///
/// ```kdl
/// mdns {
///     enable true
///     hostname "kosh-vault"
///     interfaces "eth0" "wlan0"
///     txt {
///         property "vendor" "kosh"
///     }
/// }
/// ```
#[derive(Decode, Debug, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Mdns {
    /// Byte offset span of the `mdns` node within the source configuration file.
    ///
    /// Used by `miette` diagnostics to pinpoint error locations and underline the offending
    /// configuration block in the terminal if validation fails.
    #[knuffel(span)]
    pub span: Span,

    /// Whether mDNS broadcasting is enabled.
    ///
    /// When set to `false`, the server skips mDNS broadcaster initialization, will not
    /// bind multicast UDP sockets, and will not advertise `.local` records.
    ///
    /// Defaults to `true`.
    #[knuffel(child, unwrap(argument), default = true)]
    pub enable: bool,

    /// Local network hostname prefix (advertised as `<hostname>.local`).
    ///
    /// Must conform to DNS single-label rules ([RFC 1034](https://datatracker.ietf.org/doc/html/rfc1034)):
    /// 1 to 63 ASCII alphanumeric characters or hyphens, without dots or leading/trailing hyphens.
    /// Do not specify `.local` or domain suffixes here; Kosh automatically appends `.local`.
    ///
    /// Defaults to `"kosh"`.
    #[knuffel(child, unwrap(argument), default = "kosh".into())]
    pub hostname: String,

    /// Explicit network interface names to bind and advertise across (e.g. `interfaces "eth0" "wlan0"`).
    ///
    /// If omitted or empty, the server advertises across all active, non-loopback network interfaces.
    /// If non-empty, mDNS announcements and address resolution are restricted exclusively to
    /// the listed interface names.
    #[knuffel(child, unwrap(arguments), default)]
    pub interfaces: Vec<String>,

    /// Custom DNS-SD TXT record key-value properties.
    ///
    /// Allows advertising auxiliary server metadata (e.g. protocol version, server features)
    /// to clients during service discovery.
    #[knuffel(child, default)]
    pub txt: Txt,
}

impl Default for Mdns {
    /// Constructs default mDNS settings with broadcasting enabled on all interfaces as `"kosh.local"`.
    fn default() -> Self {
        Self {
            span: Span(0, 0),
            enable: true,
            hostname: "kosh".into(),
            interfaces: Vec::new(),
            txt: Txt::default(),
        }
    }
}

impl Mdns {
    /// Validates mDNS hostname, interface names, and TXT record properties against network RFCs.
    ///
    /// Performs semantic validation on all mDNS fields:
    /// - Checks that [`hostname`](Mdns::hostname) is a valid single DNS label (RFC 1034/1123).
    /// - Checks that all declared [`interfaces`](Mdns::interfaces) have valid system interface names.
    /// - Validates custom [`txt`](Mdns::txt) properties against RFC 6763 rules and safe MTU payload limits.
    ///
    /// # Arguments
    ///
    /// * `source_name` - The file path or descriptor string of the configuration source (e.g. `"config.kdl"`).
    /// * `source_code` - The raw string contents of the configuration file, used for diagnostic rendering.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic::InvalidHostname`] if the hostname violates DNS label requirements.
    /// Returns [`Diagnostic::InvalidInterface`] if any network interface name is malformed.
    /// Returns [`Diagnostic::InvalidPropertyKey`], [`Diagnostic::ValueTooLong`], or [`Diagnostic::PayloadTooLarge`]
    /// if TXT record validation fails.
    pub fn validate(
        &self,
        source_name: &str,
        source_code: &str,
    ) -> Result<(), Diagnostic> {
        let src =
            Arc::new(NamedSource::new(source_name, source_code.to_string()));
        self.validate_hostname(&src)?;
        self.validate_interfaces(&src)?;
        self.txt.validate(&src)?;
        Ok(())
    }

    /// Validates that the hostname complies with DNS label standards (RFC 1034 / RFC 1123).
    ///
    /// The hostname must:
    /// - Not be empty.
    /// - Not exceed 63 octets (RFC 1035 label length limit).
    /// - Not contain dots (`.`) — users must not specify `.local` as Kosh handles this automatically.
    /// - Not start or end with a hyphen (`-`).
    /// - Only contain ASCII alphanumeric characters and hyphens (`[a-zA-Z0-9-]`).
    ///
    /// # Arguments
    ///
    /// * `src` - Named source reference for terminal diagnostic code snippet rendering.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic::InvalidHostname`] if the hostname is empty, exceeds 63 characters,
    /// contains dots, starts/ends with a hyphen, or contains non-alphanumeric ASCII characters.
    fn validate_hostname(
        &self,
        src: &Arc<NamedSource>,
    ) -> Result<(), Diagnostic> {
        let h = &self.hostname;
        if h.is_empty() {
            return Err(Diagnostic::InvalidHostname {
                hostname: h.clone(),
                reason: "Hostname cannot be empty",
                help: "Provide a valid single-label hostname (e.g. \"kosh\")"
                    .to_string(),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if h.len() > 63 {
            return Err(Diagnostic::InvalidHostname {
                hostname: h.clone(),
                reason: "Hostname exceeds 63 characters",
                help: format!(
                    "RFC 1035 limits DNS labels to 63 characters. Current length: {}.",
                    h.len()
                ),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if h.contains('.') {
            let suggested = h.trim_end_matches(".local").trim_end_matches('.');
            return Err(Diagnostic::InvalidHostname {
                hostname: h.clone(),
                reason: "Hostname must not contain dots (do not include `.local` or domain suffixes)",
                help: format!(
                    "Change hostname to \"{suggested}\" — Kosh automatically advertises as \"{suggested}.local\""
                ),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if h.starts_with('-') || h.ends_with('-') {
            return Err(Diagnostic::InvalidHostname {
                hostname: h.clone(),
                reason: "Hostname cannot start or end with a hyphen",
                help: "Remove leading or trailing hyphens from hostname"
                    .to_string(),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if let Some(bad_char) =
            h.chars().find(|c| !c.is_ascii_alphanumeric() && *c != '-')
        {
            return Err(Diagnostic::InvalidHostname {
                hostname: h.clone(),
                reason: "Hostname contains invalid characters",
                help: format!(
                    "DNS hostnames can only contain ASCII alphanumeric characters and hyphens [a-zA-Z0-9-]. \nOffending character: '\x1b[1m\x1b[33m{bad_char}\x1b[0m'."
                ),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        Ok(())
    }

    /// Validates declared network interface names against system limits.
    ///
    /// Each interface name must:
    /// - Not be empty or solely whitespace.
    /// - Not exceed 15 bytes (`IFNAMSIZ - 1` limit standard across POSIX / Linux kernels).
    /// - Contain only valid ASCII alphanumeric characters, hyphens, underscores, or dots.
    ///
    /// # Arguments
    ///
    /// * `src` - Named source reference for terminal diagnostic code snippet rendering.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic::InvalidInterface`] if any interface name is empty, exceeds 15 bytes,
    /// or contains characters outside `[a-zA-Z0-9-_.]`.
    fn validate_interfaces(
        &self,
        src: &Arc<NamedSource>,
    ) -> Result<(), Diagnostic> {
        for iface in &self.interfaces {
            let trimmed = iface.trim();
            if trimmed.is_empty() {
                return Err(Diagnostic::InvalidInterface {
                    interface: iface.clone(),
                    reason: "Interface name cannot be empty",
                    src: Arc::clone(src),
                    span: self.span.clone().into(),
                });
            }

            if iface.len() > 15 {
                return Err(Diagnostic::InvalidInterface {
                    interface: iface.clone(),
                    reason: "Interface name exceeds 15 bytes",
                    src: Arc::clone(src),
                    span: self.span.clone().into(),
                });
            }

            if let Some(_bad_char) = iface.chars().find(|c| {
                !c.is_ascii_alphanumeric()
                    && *c != '-'
                    && *c != '_'
                    && *c != '.'
            }) {
                return Err(Diagnostic::InvalidInterface {
                    interface: iface.clone(),
                    reason: "Interface name contains invalid characters",
                    src: Arc::clone(src),
                    span: self.span.clone().into(),
                });
            }
        }

        Ok(())
    }
}
