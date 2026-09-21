fn main() {
    let x: () = rustls::crypto::ring::default_provider().install_default();
}
