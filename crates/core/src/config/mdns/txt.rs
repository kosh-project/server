use knuffel::{Decode, span::Span};
use miette::NamedSource;
use std::sync::Arc;

use crate::config::mdns::Diagnostic::{
    self, InvalidPropertyKey, PayloadTooLarge, ValueTooLong,
};

/// Configuration for custom DNS-SD TXT record properties.
#[derive(Debug, Decode, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Txt {
    /// Byte offset span of the `txt` node within the configuration file.
    #[knuffel(span)]
    pub span: Span,

    /// Collection of custom key-value attributes.
    #[knuffel(children(name = "property"))]
    pub properties: Vec<Property>,
}

impl Default for Txt {
    fn default() -> Self {
        Self {
            span: Span(0, 0),
            properties: Vec::new(),
        }
    }
}

/// A key-value pair included in mDNS DNS-SD TXT records.
#[derive(Decode, Debug, Clone, PartialEq, Eq)]
#[knuffel(span_type = Span)]
pub struct Property {
    /// Byte offset span of this property node within the configuration file.
    #[knuffel(span)]
    pub span: Span,

    /// The attribute key.
    #[knuffel(argument)]
    pub key: String,

    /// The attribute value.
    #[knuffel(argument)]
    pub value: String,
}

impl Txt {
    /// Validates all custom TXT properties against RFC 6763 rules and safe MTU limits.
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
