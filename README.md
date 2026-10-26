<div align="center">

# Kosh Server

**The high-performance, zero-knowledge storage vault engineered for low-power hardware.**

[![CI Status](https://github.com/kosh-project/server/actions/workflows/tests.yaml/badge.svg)](https://github.com/kosh-project/server/actions/workflows/tests.yaml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Rust: 1.80+](https://img.shields.io/badge/Rust-1.80+-orange.svg)](https://www.rust-lang.org)
[![Target: Low-Power](https://img.shields.io/badge/Target-Low--Power%20%7C%20Sandy%20Bridge%20%7C%20ARM64-brightgreen.svg)](#hardware-philosophy)

[Architecture](#architecture) • [Features](#key-features) • [Hardware Philosophy](#hardware-philosophy) • [Quickstart](#quickstart) • [Contributing](CONTRIBUTING.md)

</div>

---

## Overview

**Kosh** is an offline-first, zero-knowledge personal cloud storage and digital memory platform. It is engineered to replace proprietary cloud silos while granting users total data sovereignty and cryptographic privacy.

Most cloud software assumes elastic compute, gigabytes of spare RAM, and fast NVMe SSDs. **Kosh is built under the opposite assumption**: it is designed to run with sub-millisecond responsiveness on hardware most people have already written off — such as a 2nd-generation Intel Core i5 Sandy Bridge box with 2 GB of RAM and a single spinning mechanical hard drive, or a low-power ARM single-board computer (Raspberry Pi 4 / Chromebook).

---

## Architecture: "The Dumb Bucket" vs. "The Brain"

Kosh enforces a strict separation of trust between the client applications and the backend server:

```text
[ Client Application: "The Brain" ]
  • Fully trusted edge (Android Keystore / Desktop Keychain)
  • Holds root master key (K_vault) in hardware enclave
  • Encrypts all file content, EXIF metadata, and directory trees
  • Solves Hashcash PoW challenges & performs WebAuthn biometric touch
                          │
                          │ HTTPS / TLS (Out-of-band Fingerprint Pinning)
                          ▼
[ Kosh Server: "The Dumb Bucket" ]
  • Zero Hardware Trust: treated as an untrusted remote store
  • Never sees filenames, MIME types, or plaintext bytes
  • Multi-layer ingress shield drops volumetric abuse in RAM
  • Enforces atomic POSIX storage transactions & single-writer delta ledger
```

### Ingress Defense Funnel

Every incoming request traverses a multi-stage defense funnel designed to terminate unauthorized or malicious traffic in memory before it can touch SQLite or trigger mechanical disk seeks:

```text
[ Incoming TCP Connection ]
             │
             ▼
┌────────────────────────────────────────────────────────┐
│ Stage 1: Global IP Shield (tower_governor)             │
│ • Absorbs volumetric floods in ~50ns in RAM            │
└────────────────────────────┬───────────────────────────┘
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
(Auth Routes: /api/auth/*)        (Protected Routes: /api/v1/*)
            │                                 │
            ▼                                 ▼
┌────────────────────────┐       ┌────────────────────────────────────────┐
│ Auth IP Rate Limiter   │       │ Stage 2: Stateless BLAKE3 MAC Filter   │
│ • Throttles auth spam  │       │ • Validates Bearer token HMAC in RAM   │
└───────────┬────────────┘       │ • Zero DB queries, zero cache hits     │
            │                    └────────────────────┬───────────────────┘
            ▼                                         │
┌────────────────────────┐                            ▼
│ Stateless Hashcash PoW │       ┌────────────────────────────────────────┐
│ • 16-bit difficulty    │       │ Stage 3: Authenticated Device Governor │
│ • Defends /register    │       │ • Prevents client sync loop thrashing  │
│   and /login in RAM    │       └────────────────────┬───────────────────┘
└───────────┬────────────┘                            │
            │                                         ▼
            ▼                    ┌────────────────────────────────────────┐
┌────────────────────────┐       │ Stage 4: Session Resolution            │
│ Passkey 1-Touch Login  │       │ • Fast Path: Moka RAM Cache (10m TTI)  │
│ • Biometric assertion  │       │ • Slow Path: SQLite index seek (10µs)  │
└────────────────────────┘       └────────────────────┬───────────────────┘
                                                      │
                                                      ▼
                                 ┌────────────────────────────────────────┐
                                 │ Protected Route Handlers               │
                                 │ • CAS Blob Vault & Delta-CRDT Ledger   │
                                 │ • Passkey Enrollment & ECDH Mailbox    │
                                 └────────────────────────────────────────┘
```

---

## Key Features

- **Zero-Knowledge Content-Addressable Storage (CAS):** File blobs are stored strictly by their 64-character BLAKE3 hex hash. Filenames, folder structures, and MIME types are fully client-side encrypted before upload.
- **Single-Writer Delta-CRDT WAL Sync Ledger:** An append-only Write-Ahead Log (WAL) coordinates real-time multi-device synchronization. A single background actor serializes mutating writes, while thousands of concurrent clients stream delta segments via stateless, zero-copy reads.
- **1-Touch WebAuthn Biometric Passkeys:** Hardware authenticators (Apple Touch ID / Face ID, Windows Hello, Android StrongBox, YubiKey) authenticate without passwords or emails. Uses a lossless mathematical bijection mapping SQLite `i64` user IDs directly to 16-byte WebAuthn `Uuid` handles with hardware clone detection.
- **Root Envelope Encryption & Recovery Rotation:** The master vault encryption key ($K_{\text{vault}}$) is wrapped inside a 60-byte ChaCha20-Poly1305 envelope (`wrapped_vault_key`). Recovery phrases can be rotated at any time (`PUT /api/v1/auth/recovery`), re-wrapping the master key without re-encrypting stored files.
- **Ephemeral ECDH Blind Mailbox Relay:** Newly authenticated passkey devices obtain the master key from existing devices via an out-of-band QR code key exchange over a temporary in-memory relay (Moka RAM cache with 5-minute TTL, single-use burn-after-reading, and zero disk I/O).
- **Out-of-Band TLS Certificate Pinning:** Generates self-signed X.509 certificates on first boot with SHA-256 fingerprint verification out-of-band, eliminating reliance on centralized certificate authorities on local networks.
- **Zero-Heap Error Architecture:** Compile-time static string errors (`BadRequest(&'static str)`), `wrap_internal_err!` macro bridges, and sanitized HTTP responses prevent heap allocations on error paths.
- **Terminal Admin Dashboard (TUI):** Includes a dedicated Ratatui terminal application (`crates/cli`) connecting over a local Unix Domain Socket (`/tmp/kosh.sock`) to view live telemetry, filtered log streams, and device pairing QR codes.

---

## Hardware Philosophy

Mechanical spinning hard drives hate random seeks. A few dozen concurrent un-cached reads or unindexed queries will drive seek latencies from 2 milliseconds to 800 milliseconds, starving the CPU. Kosh enforces physical hardware invariants to guarantee efficiency:

1. **Mandatory Disk Pre-Allocation:** Upload transactions pre-allocate contiguous disk sectors using `posix_fallocate` before streaming bytes from the network, preventing file fragmentation.
2. **Keyset Cursor Pagination:** All asset and ledger listing queries use composite B-Tree indexes (`idx_assets_user_tag_cursor`) with $O(\log N)$ keyset seeking. SQL `OFFSET` pagination is strictly forbidden.
3. **RAM-First Defense:** Rate limiters, HMAC token validation, and Hashcash verification run entirely in memory. Forged tokens and volumetric floods are dropped before touching the database or disk heads.

---

## Workspace Structure

The project is structured as a Cargo workspace with unified dependencies and strict Clippy lints:

| Crate | Purpose |
|---|---|
| [`kosh-core`](crates/core) | Shared domain types: KDL configuration decoding, telemetry entries, and TLS certificates. |
| [`webdav-server`](crates/server) | Production Axum server: CAS storage engine, WAL ledger actor, WebAuthn passkey handlers, and SQLite models. |
| [`cli`](crates/cli) | Real-time Ratatui terminal administrative dashboard monitoring server telemetry over Unix Domain Sockets. |

---

## Quickstart

### Prerequisites
- [Rust 1.80+](https://rustup.rs/) (Stable toolchain)
- SQLite 3
- `sqlx-cli` (for compiling migrations offline):
  ```bash
  cargo install sqlx-cli --no-default-features --features sqlite
  ```

### Build and Run

```bash
# 1. Clone the repository
git clone https://github.com/kosh-project/server.git
cd server

# 2. Build the workspace
cargo build --workspace

# 3. Run the server daemon
cargo run -p webdav-server
```

On first startup, the server automatically generates `kosh.kdl` configuration, creates the `vault/` storage directory, generates an X.509 self-signed TLS certificate, and seeds an internal server secret.

### Running the Admin TUI Dashboard

In a separate terminal window, launch the interactive monitoring dashboard:

```bash
cargo run -p cli
```

Use arrow keys to navigate logs, `/` to filter by keywords, and `p` to inspect device pairing information.

---

## Configuration (`kosh.kdl`)

Kosh uses [KDL](https://kdl.dev) for human-readable, schema-validated configuration:

```kdl
// Storage vault directory
vault-path "./vault"

// Network bindings
host "0.0.0.0"
port 23342

// TLS Configuration
enable-tls false
socket-path "/tmp/kosh.sock"

// WebAuthn Passkey Relying Party Settings
passkey {
    id "localhost"
    origin "http://localhost:23342"
    name "Kosh Vault"
}
```

---

## Quality and Verification Standards

Kosh enforces strict zero-warning engineering standards across the entire workspace:

```bash
# Verify formatting
cargo fmt --all -- --check

# Run strict Clippy lints (no unwrap, no expect, no panic, no raw as conversions)
cargo clippy --workspace --all-targets -- -D warnings

# Run all unit and integration tests
cargo test --workspace

# Verify documentation builds cleanly
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

---

## Contributing

We welcome contributions from developers interested in systems programming, cryptography, low-level I/O optimization, and clean architecture.

Please read our [**Contributor and Architecture Guide**](CONTRIBUTING.md) for detailed onboarding instructions, database migration workflows, and code review expectations. All contributors are expected to adhere to our [**Code of Conduct**](CODE_OF_CONDUCT.md).

---

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).
