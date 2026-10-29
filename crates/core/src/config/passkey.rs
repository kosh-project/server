/// Configuration settings for `WebAuthn` passkey authentication.
///
/// In the `WebAuthn` standard, the server acts as the **Relying Party (RP)**.
/// Authenticating clients verify that the RP ID and Origin requested by the server
/// match the domain where the user is browsing before allowing a credential to be
/// created or signed.
///
/// # Example KDL configuration
///
/// ```kdl
/// passkey {
///     id "example.com"
///     origin "https://example.com"
///     name "Example Vault"
/// }
/// ```
#[derive(knuffel::Decode, Debug, Clone, PartialEq, Eq)]
pub struct Passkey {
    /// Relying Party identifier (RP ID).
    ///
    /// According to the `WebAuthn` specification, this must be a valid domain string
    /// without a scheme, port, or path (e.g., `"example.com"` or `"localhost"`).
    /// Browsers restrict credentials so that an authenticator registered under this
    /// RP ID will only respond to origins matching or suffixing this domain.
    ///
    /// Defaults to `"localhost"`.
    #[knuffel(child, unwrap(argument), default = "localhost".to_string())]
    pub id: String,

    /// Canonical origin URL where the client-side authentication ceremonies occur.
    ///
    /// Must include the scheme, hostname, and port (if non-standard), such as
    /// `"http://localhost:23342"` or `"https://vault.example.com"`. Browsers strictly
    /// validate that `window.location.origin` matches this URL during both registration
    /// and login ceremonies to prevent cross-origin impersonation attacks.
    ///
    /// Defaults to `"http://localhost:23342"`.
    #[knuffel(child, unwrap(argument), default = "http://localhost:23342".to_string())]
    pub origin: String,

    /// Human-friendly name of the Relying Party displayed by client authenticators.
    ///
    /// Platforms like iOS, macOS, Windows Hello, and Android show this label when
    /// prompting the user to save or touch their passkey (e.g., `"Kosh Vault"`).
    ///
    /// Defaults to `"Kosh Vault"` when instantiated via [`Default::default`], or an
    /// empty string (`""`) if the `name` node is omitted within an explicit `passkey` block in KDL.
    #[knuffel(child, unwrap(argument), default)]
    pub name: String,
}

impl Default for Passkey {
    /// Constructs default `WebAuthn` Relying Party settings for local development.
    ///
    /// Sets `id` to `"localhost"`, `origin` to `"http://localhost:23342"`, and
    /// `name` to `"Kosh Vault"`.
    fn default() -> Self {
        Self {
            id: "localhost".to_string(),
            origin: "http://localhost:23342".to_string(),
            name: "Kosh Vault".to_string(),
        }
    }
}
