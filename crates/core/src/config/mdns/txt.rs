//! DNS-SD TXT record property configuration and validation.
//!
//! Multicast DNS Service Discovery ([RFC 6763](https://datatracker.ietf.org/doc/html/rfc6763)) allows
//! services to publish auxiliary metadata as key-value pairs formatted as `key=value` strings inside
//! DNS TXT records.
//!
//! To prevent IP packet fragmentation over lossy or low-bandwidth links (such as home Wi-Fi),
//! RFC 6763 recommends keeping total TXT record sizes well under the typical 512-byte DNS UDP
//! limit. This module enforces strict boundaries on key characters, value lengths, and total payload size.

use knuffel::{Decode, span::Span};
use miette::NamedSource;
use std::sync::Arc;

use crate::config::mdns::Diagnostic::{
    self, InvalidPropertyKey, PayloadTooLarge, ValueTooLong,
};

/// Configuration container for custom DNS-SD TXT record properties.
///
/// Under the `mdns` node in KDL, zero or more `property` entries can be specified
/// inside a child `txt` block:
///
/// ```kdl
/// mdns {
///     txt {
///         property "version" "1.0"
///         property "auth" "passkey"
///     }
/// }
/// ```
#[derive(Debug, Decode, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Txt {
    /// Byte offset span of the `txt` node within the configuration file.
    #[knuffel(span)]
    pub span: Span,

    /// Collection of custom key-value attributes to publish in the DNS-SD TXT record.
    #[knuffel(children(name = "property"))]
    pub properties: Vec<Property>,
}

impl Default for Txt {
    /// Constructs an empty TXT records container with zero custom properties.
    fn default() -> Self {
        Self {
            span: Span(0, 0),
            properties: Vec::new(),
        }
    }
}

/// A key-value attribute pair included in mDNS DNS-SD TXT records.
///
/// Encoded as `key=value` strings within the DNS-SD TXT record according to RFC 6763.
#[derive(Decode, Debug, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Property {
    /// Byte offset span of this property node within the configuration file.
    #[knuffel(span)]
    pub span: Span,

    /// The attribute key (up to 32 printable ASCII characters, excluding `=`).
    #[knuffel(argument)]
    pub key: String,

    /// The attribute value (up to 128 bytes).
    #[knuffel(argument)]
    pub value: String,
}

impl Txt {
    /// Validates all custom TXT properties against RFC 6763 rules and safe MTU limits.
    ///
    /// Validates each property individually via [`Property::validate`], and enforces that
    /// the aggregate byte length of all encoded `key=value` attributes does not exceed
    /// 400 bytes, avoiding UDP packet fragmentation across Wi-Fi networks.
    ///
    /// # Arguments
    ///
    /// * `src` - Reference to the named source code file for error span rendering.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic::InvalidPropertyKey`] or [`Diagnostic::ValueTooLong`] if any
    /// individual property is invalid.
    /// Returns [`Diagnostic::PayloadTooLarge`] if the combined TXT payload exceeds 400 bytes.
    pub(crate) fn validate(
        &self,
        src: &Arc<NamedSource>,
    ) -> Result<(), Diagnostic> {
        let mut total_len = 0;
        for p in &self.properties {
            let key = &p.key;

            p.validate(src)?;

            let val_len = p.value.len();
            total_len += key.len() + val_len + 1;
        }

        if total_len > 400 {
            return Err(PayloadTooLarge {
                total_len,
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        Ok(())
    }
}

impl Property {
    /// Validates an individual key-value property pair against RFC 6763 DNS-SD specifications.
    ///
    /// # Validation Rules
    ///
    /// - **Key non-empty**: The key string must contain at least 1 character.
    /// - **Key length limit**: The key must not exceed 32 characters (RFC 6763 Section 6.4 recommends short keys).
    /// - **Key delimiter**: The key must not contain `=` since `=` is the DNS-SD key-value separator (RFC 6763 Section 6.4).
    /// - **Key character set**: The key must contain only printable ASCII characters (0x20 through 0x7E).
    /// - **Value length limit**: The value string must not exceed 128 bytes.
    ///
    /// # Arguments
    ///
    /// * `src` - Reference to the named source code file for error span rendering.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic::InvalidPropertyKey`] if the key is empty, too long, contains `=`, or has non-printable characters.
    /// Returns [`Diagnostic::ValueTooLong`] if the value exceeds 128 bytes.
    #[inline]
    fn validate(&self, src: &Arc<NamedSource>) -> Result<(), Diagnostic> {
        let key = &self.key;
        if key.is_empty() {
            return Err(InvalidPropertyKey {
                key: key.into(),
                reason: "Property key cannot be empty",
                help: "Specify a non-empty key conforming to RFC 6763 DNS-SD"
                    .to_string(),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if key.len() > 32 {
            return Err(InvalidPropertyKey {
                key: key.clone(),
                reason: "Key exceeds 32 characters",
                help: format!(
                    "RFC 6763 recommends short keys. Shorten `{key}` (currently {} bytes).",
                    key.len()
                ),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if key.contains('=') {
            return Err(InvalidPropertyKey {
                key: key.clone(),
                reason: "Key cannot contain '=' character",
                help: "In DNS-SD TXT records, '=' is reserved as the key-value delimiter".to_string(),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        if let Some(bad_byte) =
            key.bytes().find(|&b| !(0x20..=0x7E).contains(&b))
        {
            return Err(InvalidPropertyKey {
                key: key.clone(),
                reason: "Key contains non-printable ASCII characters",
                help: format!(
                    "RFC 6763 requires property keys to contain only printable ASCII (0x20..=0x7E). Offending byte: 0x{bad_byte:02X}."
                ),
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        let val_len = self.value.len();
        if val_len > 128 {
            return Err(ValueTooLong {
                key: key.clone(),
                actual_len: val_len,
                src: Arc::clone(src),
                span: self.span.clone().into(),
            });
        }

        Ok(())
    }
}
