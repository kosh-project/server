use tokio::io;

/// Errors that can occur during TLS certificate operations.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A filesystem I/O error occurred while reading or writing PEM files.
    ///
    /// This variant is produced when `cert.pem` or `key.pem` cannot be read from or
    /// written to the `<vault_path>/tls/` directory.
    #[error("Failed to read or write TLS files: {}", .0)]
    Io(#[from] io::Error),

    /// The `rcgen` library failed to generate a self-signed X.509 certificate.
    ///
    /// This is unlikely to occur in practice but can happen if the system's random
    /// number generator is unavailable or if the certificate parameters are invalid.
    #[error("Failed to generate X.509 certificate: {}", .0)]
    CertificateGen(#[from] rcgen::Error),

    /// The PEM data did not contain any valid DER certificate blocks.
    ///
    /// Produced by [`super::Identity::fingerprint_raw`] and [`super::Identity::fingerprint`]
    /// when the `cert_pem` field cannot be parsed into at least one certificate.
    #[error("No certificate(s) found")]
    NoCertificatesFound,
}

/// A type alias for `Result<T, tls::Error>`.
pub type Result<T> = core::result::Result<T, Error>;
