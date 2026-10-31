<div align="center">

# Kosh

**High-performance, zero-knowledge storage and sync vault daemon engineered for bare-metal hardware.**

[![CI Status](https://github.com/kosh-project/server/actions/workflows/tests.yaml/badge.svg)](https://github.com/kosh-project/server/actions/workflows/tests.yaml)
[![Docker Builds](https://github.com/kosh-project/server/actions/workflows/docker.yaml/badge.svg)](https://github.com/kosh-project/server/actions/workflows/docker.yaml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust Edition](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org)
[![Target: Low-Power Systems](https://img.shields.io/badge/Target-Low--Power%20%7C%20Sandy%20Bridge%20%7C%20ARM64-brightgreen.svg)](#hardware-philosophy)
[![Security Policy](https://img.shields.io/badge/Security-Policy-red.svg)](SECURITY.md)

[Architecture](#architecture-the-dumb-bucket-vs-the-brain) • [Key Primitives](#core-architectural-primitives) • [Hardware Philosophy](#hardware-philosophy) • [Quickstart](#quickstart) • [Configuration](#configuration-configkdl) • [Contributing](CONTRIBUTING.md)

</div>

---

## What is Kosh?

**Kosh** is an end-to-end encrypted, zero-knowledge personal storage and synchronization engine written in Rust. It functions as an autonomous, self-hosted vault daemon that safeguards your digital memory, media, and documents across multiple client devices.

Most private cloud solutions assume elastic cloud infrastructure: gigabytes of idle RAM, multi-core virtual machines, and fast NVMe solid-state storage. When deployed on personal hardware, they often choke under high memory consumption and constant database locks.

**Kosh takes the opposite approach.** It is engineered specifically to deliver sub-millisecond local network response times on minimal hardware: a ten-year-old Intel Core i5 Sandy Bridge machine with 2 GB of RAM and a single mechanical hard drive, or a low-power ARM single-board computer.

The server operates strictly as an untrusted, blind **"Dumb Bucket of Bits"**. It stores, deduplicates, and coordinates opaque ciphertext without ever seeing your filenames, directory hierarchies, file contents, or personal identity.

---

## Architecture: "The Dumb Bucket" vs. "The Brain"

Kosh establishes a rigid cryptographic trust boundary between client applications and the backend storage daemon:

```text
[ Client Applications: "The Brain" ]
  • Runs on trusted client hardware (Android Keystore, Apple Secure Enclave, Desktop Keychain)
  • Retains the root master vault key (K_vault) strictly client-side
  • Encrypts and decrypts all payload bytes, EXIF metadata, and directory trees
  • Solves Hashcash Proof-of-Work challenges and conducts WebAuthn biometric assertions
                              │
                              │ HTTPS / TLS (Out-of-Band Fingerprint Pinning)
                              ▼
[ Kosh Server Daemon: "The Dumb Bucket" ]
  • Zero Hardware Trust: treated as a hostile, untrusted remote store
  • Never inspects filenames, MIME types, or unencrypted bytes
  • Multi-stage memory-first ingress shield drops abuse in RAM within nanoseconds
  • Coordinates atomic Content-Addressable Storage (CAS) and append-only WAL ledgers
```

### Memory-First Ingress Defense Funnel

Spinning hard drives degrade rapidly under random I/O operations. To protect physical disk heads and async Tokio worker threads from starvation, every incoming request passes through an in-memory ingress filter:

```text
[ Incoming TCP / TLS Connection ]
                │
                ▼
┌────────────────────────────────────────────────────────┐
│ Stage 1: Global IP Governor (tower_governor)           │
│ • Absorbs volumetric floods in ~50 nanoseconds in RAM  │
└───────────────────────────┬────────────────────────────┘
                            │
            ┌───────────────┴───────────────┐
            ▼                               ▼
(Auth Routes: /api/auth/*)      (Protected Routes: /api/v1/*)
            │                               │
            ▼                               ▼
┌────────────────────────┐      ┌────────────────────────────────────────┐
│ Auth IP Rate Limiter   │      │ Stage 2: Stateless BLAKE3 MAC Filter   │
│ • Throttles auth abuse │      │ • Validates Bearer token HMAC in RAM   │
└───────────┬────────────┘      │ • Drops forged tokens with zero disk   │
            │                   │   or database overhead (<1 microsecond)│
            ▼                   └───────────────────┬────────────────────┘
┌────────────────────────┐                          │
│ Stateless Hashcash PoW │                          ▼
│ • 16-bit difficulty    │      ┌────────────────────────────────────────┐
│ • Defends /register    │      │ Stage 3: Device Sync Token Governor    │
│   and /login in RAM    │      │ • Throttles runaway client sync loops  │
└───────────┬────────────┘      └───────────────────┬────────────────────┘
            │                                       │
            ▼                                       ▼
┌────────────────────────┐      ┌────────────────────────────────────────┐
│ WebAuthn Passkey Login │      │ Stage 4: Session Resolution            │
│ • Biometric assertion  │      │ • Fast Path: Moka RAM Cache (10m TTI)  │
│ • Zero PoW required    │      │ • Slow Path: SQLite index seek (10µs)  │
└────────────────────────┘      └───────────────────┬────────────────────┘
                                                    │
                                                    ▼
                                ┌────────────────────────────────────────┐
                                │ Protected Route Handlers               │
                                │ • CAS Storage Vault (Atomic Blobs)     │
                                │ • WAL Delta-CRDT Synchronization Log   │
                                │ • Passkey Enrollment & ECDH Mailbox    │
                                └────────────────────────────────────────┘
```

---


## Quickstart

### Option 1: Build from source

#### Prerequisites
- [Rust 1.97.1+](https://rustup.rs/) (Stable toolchain, 2024 edition compatible)
- SQLite 3
- `sqlx-cli` (for compiling migrations offline):
  ```bash
  cargo install sqlx-cli --no-default-features --features sqlite
  ```

#### Build and Run
```bash
# Clone the repository
git clone https://github.com/kosh-project/server.git
cd server

# Build the workspace
cargo build --workspace --release
```
```bash
# Launch the server daemon
cargo run -p server

# Or see the logs via tui
cargo run -p cli
```
#### Or Install them
```bash
cargo install --path crates/server

cargo install --path crates/cli

# and run them with `kosd` or `kosh-cli`
```
On initial startup, Kosh generates the default configuration file (`config/config.kdl` or `kosh.kdl`), provisions the `vault/` storage directory, generates an X.509 self-signed TLS certificate, and seeds an internal server secret.

---

### Option 2: Docker / Container Deployment

Kosh provides multi-arch container images running under an unprivileged service account (`UID 10001`):

```bash
# Run with host networking for mDNS LAN discovery
docker run -d \
  --name kosh-server \
  --network host \
  -v /var/lib/kosh/vault:/vault \
  -v /etc/kosh:/etc/kosh \
  -v /run/kosh:/tmp \
  ghcr.io/kosh-project/server:latest
```

Using Docker Compose (pre-configured in the repository via [`docker-compose.yml`](docker-compose.yml)):

```bash
docker compose up -d
```

Compose configuration :

```yaml
services:
  kosh-server:
    image: ghcr.io/kosh-project/server:latest
    container_name: kosh-server
    network_mode: host
    restart: unless-stopped
    environment:
      - KOSH_CONFIG=/etc/kosh/config.kdl
      - RUST_LOG=info
    volumes:
      - ${KOSH_CONFIG_PATH:-./config/config.kdl}:/etc/kosh/config.kdl:ro
      - ${KOSH_VAULT_PATH:-./vault}:/vault:rw
      - ${KOSH_RUN_PATH:-./run}:/tmp:rw
```

---

## Configuration (`config.kdl`)

Kosh uses [KDL](https://kdl.dev) for config, (see [example.]):

```kdl
// Storage vault directory
vault-path "./vault"

// Network bindings
host "0.0.0.0"
port 23342

// Transport security (Rustls)
enable-tls false
socket-path "/tmp/kosh.sock"

// WebAuthn Passkey Relying Party
passkey {
    id "localhost"
    origin "http://localhost:23342"
    name "Kosh Vault"
}

// Multicast DNS LAN Discovery
mdns {
    enable true
    hostname "kosh"
    txt {
        property "vendor" "kosh"
        property "version" "0.1.4"
    }
}
```

---
## Community

We are committed to building an open, welcoming, and high-performance engineering environment:

- **Contributing Guide:** Read [CONTRIBUTING.md](CONTRIBUTING.md) for full architectural specifications, database migration workflows, and development guidelines.
- **Code of Conduct:** Read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for our community standards and enforcement policies.
- **Security Policy:** Read [SECURITY.md](SECURITY.md) to report vulnerabilities privately.
- **Support:** Visit [SUPPORT.md](SUPPORT.md) for technical assistance and architecture discussions.

---

## License

This project is licensed under the [Apache License, Version 2.0](LICENSE).
