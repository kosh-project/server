fn main() {
    rustls::crypto::ring::default_provider().install_default().unwrap();
}
