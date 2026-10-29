use knuffel::{Decode, span::Span};
use miette::NamedSource;
use std::sync::Arc;

use super::diagnostic::Diagnostic;
use super::txt::Txt;

/// Configuration for Multicast DNS (mDNS / DNS-SD) zero-config discovery.
#[derive(Decode, Debug, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Mdns {
    /// Byte offset span of the `mdns` node within the configuration file.
    #[knuffel(span)]
    pub span: Span,

    /// Whether mDNS broadcasting is enabled. Defaults to `true`.
    #[knuffel(child, unwrap(argument), default = true)]
    pub enable: bool,

    /// Local network hostname (advertised as `<hostname>.local`). Defaults to `"kosh"`.
    #[knuffel(child, unwrap(argument), default = "kosh".into())]
    pub hostname: String,

    /// Network interfaces to bind to (e.g. `interfaces "eth0" "wlan0"`).
    /// If omitted or empty, broadcasts on all available network interfaces.
    #[knuffel(child, unwrap(arguments), default)]
    pub interfaces: Vec<String>,

    /// Custom key-value TXT record attributes.
    #[knuffel(child, default)]
    pub txt: Txt,
}

impl Default for Mdns {
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
    /// Validates mDNS hostname, interface names, and TXT record properties.
    ///
    /// # Errors
    ///
    /// Returns a [`Diagnostic`] if any configuration field violates RFC specifications
    /// or safe packet size limits.
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
