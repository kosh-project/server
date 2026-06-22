pub(crate) struct User {
    pub id: i64,
    pub identity_hash: [u8; 32],
    pub auth_verifier: String,
}
