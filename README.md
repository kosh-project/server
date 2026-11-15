# Kosh

Zero-knowledge storage and synchronization daemon engineered for low-power bare metal.

[![CI Status](https://github.com/kosh-project/server/actions/workflows/tests.yaml/badge.svg)](https://github.com/kosh-project/server/actions/workflows/tests.yaml)
[![Docker Builds](https://github.com/kosh-project/server/actions/workflows/docker.yaml/badge.svg)](https://github.com/kosh-project/server/actions/workflows/docker.yaml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Security Policy](https://img.shields.io/badge/Security-Policy-red.svg)](SECURITY.md)

[Overview](#what-is-kosh) • [Hardware Target](#hardware-target) • [Architecture](#architecture-the-dumb-bucket-vs-the-brain) • [Ingress Shield](#memory-first-ingress-defense) • [Storage Engine](#storage-and-wal-physics) • [Quickstart](#quickstart) • [Docker](#docker-deployment) • [Configuration](#configuration) • [Community](#community)

---

## What is Kosh

Kosh is an end-to-end encrypted, zero-knowledge storage vault and synchronization engine written in Rust. It runs as an autonomous, self-hosted daemon that stores, deduplicates, and synchronizes encrypted digital memory and files across client devices.

Most private cloud backends assume modern cloud infrastructure: multi-gigabyte memory footprints, multi-core virtual machines, and fast NVMe storage. When deployed on personal commodity hardware, random I/O from unbuffered writes and database locks frequently thrash disk heads, leading to high latency and resource exhaustion.

Kosh is designed from the physical disk layer up for resource-constrained bare metal: a 2nd-generation Intel Core i5 Sandy Bridge machine with 2 GB of RAM and a single mechanical hard drive, or a low-power ARM single-board computer.

The server operates strictly as an untrusted, blind **"Dumb Bucket of Bits"**. It stores and streams opaque ciphertext without ever observing filenames, directory structures, MIME types, or personal identity. All encryption, key derivation, and metadata operations execute strictly client-side.

---

## Hardware Target

Kosh is tested against physical commodity hardware rather than elastic cloud instances:

- **Intel Core i5-2400 (Sandy Bridge):** 4 cores @ 3.1 GHz, 2 GB DDR3 RAM, 1x 7200 RPM mechanical HDD (ext4)
- **MediaTek MT8183 (ARM64):** 8 cores, 4 GB LPDDR4X RAM, eMMC 5.1 storage

### Systems-Level Invariants

Mechanical drives degrade severely under random seeks. To ensure sustained throughput on rotational media:

- **Pre-Allocated Disk Sectors:** The server calls `posix_fallocate` via Tokio before streaming payload bytes to reserve contiguous physical disk blocks. Chunked transfers lacking a valid `Content-Length` header are rejected immediately with HTTP 400.
- **RAM-First Defense Funnel:** Volumetric traffic, forged Bearer tokens, and unauthenticated brute-force requests are dropped in RAM before triggering database queries or disk seeks.
- **Two-Level Directory Prefix Sharding:** Stored blobs are partitioned across 65,536 leaf directories (`vault/ab/cd/<hash>`), eliminating ext4 HTree index depth degradation and directory inode lock (`i_rwsem`) contention.
- **Keyset Cursor Pagination:** Query paths use indexed keyset seeking (`WHERE (last_modified, id) < (?, ?)`) instead of SQL `OFFSET` scans.
- **Zero-Heap Error Paths:** Transient errors use compile-time static strings (`&'static str`) to avoid memory allocator churn under high load.

---

## Architecture: "The Dumb Bucket" vs. "The Brain"

Kosh enforces a strict cryptographic trust boundary between client applications and the storage daemon:

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
  • Zero Hardware Trust: treated as an untrusted, potentially compromised remote store
  • Never inspects filenames, MIME types, or unencrypted bytes
  • Multi-stage memory-first ingress shield drops abuse in RAM within nanoseconds
  • Coordinates 2-level sharded CAS storage and append-only WAL ledgers
```

### Responsibility Split

| Responsibility | Client Application ("The Brain") | Kosh Server ("The Dumb Bucket") |
|---|---|---|
| **Runtime** | Android (Kotlin / Jetpack) / Desktop client | Headless Linux, Axum 0.8, Tokio async runtime |
| **Trust Boundary** | Trusted edge; holds private master keys | Untrusted remote store; zero plaintext visibility |
| **Storage Semantics** | Relational metadata index, Delta-CRDT action log | Content-Addressable Storage (CAS), append-only WAL segments |
| **Data Visibility** | Plaintext files, EXIF tags, directory hierarchies | Opaque ciphertext blobs and encrypted delta frames |
| **Cryptography** | Argon2id KDF, ChaCha20-Poly1305, AES-GCM, Tink AEAD | Stateless BLAKE3 MAC verification, Hashcash verification |

---

## Memory-First Ingress Defense

Every incoming TCP connection passes through an in-memory defense pipeline before any disk or database resource is allocated:

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

## Storage and WAL Physics

### Content-Addressable Storage (CAS)
Files uploaded to the storage engine are indexed by their BLAKE3 cryptographic hash:

- **Two-Level Sharding:** Blobs are written to `vault/<hash[0..2]>/<hash[2..4]>/<hash>` (e.g. `vault/3f/8a/3f8a4b...`). This guarantees no single directory exceeds a few hundred entries, keeping filesystem lookups in O(1) on ext4 and Btrfs.
- **Upload Transaction Lifecycle:**
  1. Incoming byte stream writes to a temporary staging file: `vault/<uuid>.tmp`.
  2. `posix_fallocate` reserves contiguous disk sectors on the filesystem.
  3. Bytes are streamed in 64 KB chunks through `blake3::Hasher`.
  4. Stream completion verifies total bytes match `Content-Length`.
  5. The file is atomically renamed to its destination sharded path via `rename(2)`.
  6. SQLite metadata registers user ownership.
- **Abort Cleanliness:** Disconnections, short reads, or hash mismatches immediately unlink the staging `.tmp` file. Partial writes never accumulate.

### Write-Ahead Log (WAL) Ledgers
Client synchronization uses append-only WAL segments under `vault/ledgers/<user_id>/`:

- **Single-Writer Actor:** Mutating actions are serialized through a dedicated Tokio actor channel (`mpsc::Sender<Action>`), avoiding file locking overhead.
- **Stateless Zero-Copy Reads:** Reading delta segments bypasses the writer actor. Sync clients open segment descriptors directly, seek past the 500-byte header, and stream frames concurrently.
- **Rotation:** Segments automatically rotate at 5 MB boundaries, updating the atomic `CURRENT` pointer.

---

## Quickstart

### Prerequisites

- [Rust 1.97.1+](https://rustup.rs/) (Stable toolchain, 2024 edition compatible)
- SQLite 3
- `sqlx-cli` (optional, for compiling database migrations offline):
  ```bash
  cargo install sqlx-cli --no-default-features --features sqlite
  ```

### Build from Source

```bash
# Clone the repository
git clone https://github.com/kosh-project/server.git
cd server

# Build the workspace in release mode
cargo build --workspace --release

# Run the server daemon
cargo run -p server

# Run the administrative Ratatui TUI monitor
cargo run -p cli
```

### Local Cargo Installation

```bash
# Install the server daemon binary (installed as 'server')
cargo install --path crates/server

# Install the admin CLI binary (installed as 'cli')
cargo install --path crates/cli
```

On initial launch, Kosh creates default configurations, provisions the `vault/` hierarchy, generates self-signed TLS certificates, and seeds the root cryptographic secrets.

---

## Docker Deployment

Pre-built multi-architecture container images are available on GitHub Container Registry:

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

### Docker Compose

Run using the repository [`docker-compose.yml`](docker-compose.yml):

```bash
docker compose up -d
```

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

## Configuration

Kosh is configured via [KDL](https://kdl.dev). See the complete documented reference in [`config/config.example.kdl`](config/config.example.kdl).

```kdl
// Storage vault root path
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

- **Contributing Guide:** [CONTRIBUTING.md](CONTRIBUTING.md) details architectural patterns, coding standards, and testing workflows.
- **Security Policy:** [SECURITY.md](SECURITY.md) provides instructions for reporting vulnerabilities privately.
- **Code of Conduct:** [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) outlines our community standards.
- **Support:** [SUPPORT.md](SUPPORT.md) covers diagnostics, deployment questions, and issue reporting.

---

## License

Kosh is distributed under the [Apache License, Version 2.0](LICENSE).
