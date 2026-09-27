mod secret;
pub use secret::Secret;

pub const TOKEN_LEN: usize = 36 + 16 + 64 + 2;
