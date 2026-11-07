use std::fmt;

pub struct Token {
    pub(crate) hex: String,
    pub(crate) hash: [u8; 32],
}

impl Token {
    pub(crate) fn new() -> Self {
        let bytes: [u8; 32] = rand::random();
        let token = hex::encode(bytes);
        let hash = *blake3::hash(token.as_bytes()).as_bytes();
        Self { hex: token, hash }
    }

    #[inline]
    pub(crate) fn hash_raw(raw: &str) -> [u8; 32] {
        *blake3::hash(raw.as_bytes()).as_bytes()
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***")
    }
}

pub struct Receipt {
    pub id: String,
    pub token: Token,
}

impl Receipt {
    pub(crate) const fn new(id: String, token: Token) -> Self {
        Self { id, token }
    }
}
