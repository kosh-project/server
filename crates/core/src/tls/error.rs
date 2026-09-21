use tokio::io;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to read or write TLS files: {}", .0)]
    Io(#[from] io::Error),

    #[error("Failed to generate X.509 certificate: {}", .0)]
    CertificateGen(#[from] rcgen::Error),

    #[error("No certificate(s) found")]
    NoCertificatesFound,
}

pub type Result<T> = core::result::Result<T, Error>;
