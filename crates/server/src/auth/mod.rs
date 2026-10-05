mod secret;
pub use secret::Secret;

/// The exact ASCII length of a structured bearer token in bytes (118 bytes).
///
/// Composed of:
/// - 36 bytes: UUID v4 session ID string
/// - 1 byte: `.` delimiter
/// - 16 bytes: Lowercase hexadecimal Unix timestamp (seconds) for `expires_at`
/// - 1 byte: `.` delimiter
/// - 64 bytes: Lowercase hexadecimal keyed BLAKE3 MAC over `"session_id.expires_at_hex"`
pub const TOKEN_LEN: usize = 36 + 16 + 64 + 2;
