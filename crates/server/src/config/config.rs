use crate::error::boot;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(knuffel::Decode, Debug)]
pub struct Config {
    #[knuffel(child, unwrap(argument), default = default_vault_path())]
    pub vault_path: PathBuf,

    #[knuffel(child, unwrap(argument), default = 6969)]
    pub port: u16,

    #[knuffel(child, unwrap(argument), default = "0.0.0.0".to_string())]
    pub host: String,

    #[knuffel(child, unwrap(argument), default = false)]
    pub enable_tls: bool,

    #[knuffel(child, unwrap(argument), default = "/tmp/kosh.sock".to_string())]
    pub socket_path: String,
}

fn default_vault_path() -> PathBuf {
    PathBuf::from("./vault")
}

impl Config {
    pub async fn load_or_init(path: &Path) -> Result<Self, boot::Error> {
        if !path.exists() {
            let default_kdl = r#"// Kosh Server Configuration

// The root directory where all blobs, databases, and keys are stored.
vault-path "./vault"

// Network bindings
host "0.0.0.0"
port 6969

// Security
enable-tls false

// Admin CLI socket
socket-path "/tmp/kosh.sock"
"#;

            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).await?;
                }
            }
            fs::write(path, default_kdl).await?;
        }

        let config_txt = fs::read_to_string(path).await?;
        let config = knuffel::parse(&path.to_string_lossy(), &config_txt)?;
        Ok(config)
    }
}
