# Contributing to Kosh Server

Welcome to the backend of the Kosh platform. Whether you are here to optimize I/O paths, harden our cryptographic protocols, add database features, or inspect how a zero-knowledge cloud engine functions under the hood, we are thrilled to have you.

This document is the authoritative onboarding manual and engineering reference for contributors to `kosh-project/server`. It covers our system philosophy, crate layout, cryptographic mechanics, wire protocols, storage physics, testing workflows, and engineering standards.

---

## Table of Contents
1. [System Philosophy: The Dumb Bucket vs. The Brain](#1-system-philosophy-the-dumb-bucket-vs-the-brain)
2. [Physical Hardware Realities and I/O Mechanics](#2-physical-hardware-realities-and-io-mechanics)
3. [Workspace Layout and Crate Anatomy](#3-workspace-layout-and-crate-anatomy)
4. [High-Level Ingress Funnel](#4-high-level-ingress-funnel)
5. [Cryptographic Architecture and Invariants](#5-cryptographic-architecture-and-invariants)
6. [Storage Vault and Transaction Physics](#6-storage-vault-and-transaction-physics)
7. [Write-Ahead Log (WAL) Ledger Engine](#7-write-ahead-log-wal-ledger-engine)
8. [Database and Keyset Cursor Pagination](#8-database-and-keyset-cursor-pagination)
9. [Zero-Heap Error Architecture](#9-zero-heap-error-architecture)
10. [Telemetry and The Ratatui Admin CLI](#10-telemetry-and-the-ratatui-admin-cli)
11. [Development Setup and Workflow](#11-development-setup-and-workflow)
12. [Coding Standards and Lint Contract](#12-coding-standards-and-lint-contract)

---

## 1. System Philosophy: "The Dumb Bucket" vs. "The Brain"

Kosh is an offline-first, zero-knowledge personal cloud storage and digital memory platform. It is engineered to replace proprietary cloud storage silos while granting users complete data sovereignty and cryptographic privacy.

To achieve this, the architecture enforces a strict division of labor between client applications (e.g., Android, Desktop) and the backend server:

| Dimension | Client Applications ("The Brain") | Rust/Axum Backend ("The Dumb Bucket") |
|---|---|---|
| **Runtime Environment** | Android Edge Runtime / Desktop Client | Headless Linux service, Axum 0.8, Tokio async runtime |
| **Target Hardware** | Modern mobile SoCs and laptops | 2nd-gen Intel Core i5 (Sandy Bridge), 2–4 GB RAM, spinning HDD |
| **Trust Boundary** | Fully trusted edge; holds private master encryption keys | Zero Hardware Trust; treated as an untrusted, potentially compromised remote store |
| **Storage Semantics** | Relational metadata index, Delta-CRDT action log, Lamport clocks | Content-Addressable Storage (CAS) keyed by BLAKE3 hash, append-only WAL segment ledger |
| **Data Visibility** | Plaintext files, EXIF metadata, directory trees exist in memory only | Opaque ciphertext blobs and encrypted action frames; server never knows file names or types |
| **Cryptographic Role** | Argon2id KDF, Hashcash PoW solver, AES-256-GCM / ChaCha20-Poly1305, Tink `StreamingAead` | Stateless BLAKE3 MAC verification (`mac_guard`), Hashcash verification (`pow_guard`), session token issuance |

---

## 2. Physical Hardware Realities and I/O Mechanics

Most cloud software is written assuming infinite NVMe read/write IOPS and elastic compute. **Kosh is built under the opposite assumption.** 

Our baseline reference box is a 2nd-generation Intel Core i5 Sandy Bridge machine with 2 to 4 GB of DDR3 RAM and a single spinning mechanical hard drive. Mechanical drives degrade severely under random seeks. A few dozen concurrent un-cached reads or unbounded queries will thrash disk read heads, driving latencies from 2 milliseconds to 800 milliseconds and starving the CPU.

Because of this physical reality, the server enforces strict physical constraints:
- **Mandatory `Content-Length`:** The server uses `posix_fallocate` via Tokio to pre-allocate contiguous disk sectors before streaming bytes. Chunked transfer encoding (`Transfer-Encoding: chunked`) is immediately rejected with HTTP 400 (`BadRequest("Missing content length in header")`).
- **RAM-First Defense Funnel:** Volumetric traffic, invalid tokens, and bot brute-force attempts are terminated entirely in RAM before they can trigger database queries or disk seeks.
- **Keyset Seeking Over `OFFSET`:** We never use SQL `OFFSET` pagination. Every listing query is indexed to seek directly to the target record in $O(\log N)$ time.
- **Zero-Heap Error Paths:** Error paths use compile-time static strings (`&'static str`). Volumetric failures do not churn heap memory or trigger allocator locks.

---

## 3. Workspace Layout and Crate Anatomy

The repository is organized as a Cargo workspace with three primary crates:

```text
kosh/
├── Cargo.toml                  # Workspace root, unified dependencies and strict Clippy lints
├── crates/
│   ├── core/                   # kosh-core: shared types used by both server and CLI
│   │   ├── src/
│   │   │   ├── config/         # KDL configuration decoding (vault path, host, port, TLS, passkey)
│   │   │   ├── logger/         # Shared telemetry types (Entry, Telemetry, Level, Module)
│   │   │   ├── tls/            # X.509 certificate loading, generation, and raw fingerprints
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── server/                 # webdav-server: Production Axum HTTP/HTTPS service
│   │   ├── migrations/         # SQLx SQLite database migrations
│   │   ├── src/
│   │   │   ├── api/            # Route handlers, middleware, and API error definitions
│   │   │   │   ├── assets/     # Content-addressable binary blob upload/stream/delete
│   │   │   │   ├── auth/       # Registration, login, recovery rotation, passkey ceremonies
│   │   │   │   ├── devices/    # Cross-device sync (ephemeral ECDH blind mailbox)
│   │   │   │   ├── middleware/ # MAC verification, auth guard, rate limiters, telemetry
│   │   │   │   ├── sync/       # Delta-CRDT WAL sync endpoints (append, stream, prune)
│   │   │   │   └── route.rs    # Top-level Axum router composition
│   │   │   ├── app/            # AppState, AppStateBuilder, and in-memory challenge caches
│   │   │   ├── auth/           # Cryptographic Secret management (K_server, K_ephemeral)
│   │   │   ├── error/          # Static error types and wrap_internal_err! macro
│   │   │   ├── logger/         # Asynchronous logger service, bincode writer, UDS socket
│   │   │   ├── model/          # SQLite domain entities (User, Session, Asset, Passkey)
│   │   │   ├── storage/        # CAS storage engine and WAL segment ledger actor
│   │   │   ├── lib.rs          # Reusable server library (imported by tests)
│   │   │   ├── main.rs         # Production daemon entry point and boot sequence
│   │   │   └── server.rs       # Axum launcher (plain HTTP vs Rustls HTTPS)
│   │   ├── tests/              # End-to-end integration test suite
│   │   └── Cargo.toml
│   │
│   └── cli/                    # cli: Admin Ratatui TUI dashboard
│       ├── src/
│       │   ├── core/           # Pairing logic (BIP-39 words, QR generator, IP discovery)
│       │   ├── ui/             # Ratatui widgets (log stream, filter bar, pairing popup)
│       │   └── main.rs         # TUI event loop and UDS socket consumer
│       └── Cargo.toml
│
└── .agents/                    # Architecture blueprints and agent specifications
```

---

## 4. High-Level Ingress Funnel

Every request entering the Kosh server traverses a hardened multi-layer ingress funnel:

```text
[ Incoming TCP Connection ]
             │
             ▼
┌────────────────────────────────────────────────────────┐
│ Stage 1: Global IP Shield (tower_governor)             │
│ • Key: Peer Client IP                                  │
│ • Drops volumetric floods in ~50ns in RAM              │
└────────────────────────────┬───────────────────────────┘
                             │
                             ├─────────────────────────────────────────┐
                             │                                         │
               (Auth Routes: /api/auth/*)                (Protected Routes: /api/v1/*)
                             │                                         │
                             ▼                                         ▼
┌────────────────────────────────────────┐ ┌────────────────────────────────────────┐
│ Auth IP Governor (tower_governor)      │ │ Stage 2: Stateless BLAKE3 MAC Filter   │
│ • Key: Client IP                       │ │ • Checks Bearer token format & HMAC    │
│ • Quota throttles auth flood attacks   │ │ • Validates expires_at in RAM (<1µs)   │
└────────────────────────────┬───────────┘ │ • Rejects forged tokens instantly      │
                             │             │ • Zero DB queries, zero cache hits     │
                             │             └───────────────────┬────────────────────┘
                             │                                 │
            ┌────────────────┴────────────────┐                ▼
            ▼                                 ▼   ┌────────────────────────────────────────┐
┌────────────────────────┐       ┌────────────────┐│ Stage 3: Authenticated Device Governor │
│ Stateless Hashcash PoW │       │ Passkey Login  ││ • Key: TokenHash (BLAKE3 of Bearer)    │
│ • Validates X-Hashcash │       │ • Hardware     ││ • Prevents client sync loop thrashing  │
│   16-bit difficulty    │       │   biometric    │└───────────────────┬────────────────────┘
│ • Defends /register    │       │   assertion    │                    │
│   and /login           │       │ • Zero PoW req │                    ▼
└───────────┬────────────┘       └────────┬───────┘┌────────────────────────────────────────┐
            │                             │        │ Stage 4: Session Resolution            │
            ▼                             ▼        │ • Fast Path: Moka Cache (10-min TTI)   │
┌─────────────────────────────────────────────────┐│ • Slow Path: SQLite query (10-30µs)    │
│ Auth Handlers (/api/auth/*)                     ││ • Injects resolved user_id             │
│ • POST /register, POST /login                   │└───────────────────┬────────────────────┘
│ • POST /passkey/login/start & /finish           │                    │
└─────────────────────────────────────────────────┘                    ▼
                                                   ┌────────────────────────────────────────┐
                                                   │ Protected Handlers (/api/v1/*)         │
                                                   │ • CAS Blob Vault & Assets              │
                                                   │ • WAL Sync Ledger & Recovery Rotation  │
                                                   │ • Passkey Enrollment & ECDH Mailbox    │
                                                   └────────────────────────────────────────┘
```

---

## 5. Cryptographic Architecture and Invariants

Kosh relies on BLAKE3, ChaCha20-Poly1305, Argon2id, and WebAuthn Level 3 for cryptographic security.

### 5.1 Cryptographic Key Separation
Held in `AppState` are two distinct 32-byte cryptographic keys (`Secret`):
1. **$K_{\text{server}}$ (`state.secret`):** Loaded from or generated into `vault/server.secret` with `0o600` permissions. It persists across reboots so authenticated mobile sessions remain valid for their 30-day lifetime. Used to sign and verify Bearer tokens.
2. **$K_{\text{ephemeral}}$ (`state.pow_secret`):** Generated fresh via `rand::random()` directly into RAM on every server boot. It never touches disk. Used exclusively to sign 15-second Hashcash Proof-of-Work challenges. When the server restarts, all outstanding challenges become invalid immediately.

### 5.2 Bearer Token Structure (118 Bytes)
Bearer tokens are compact, structured, ASCII strings:
```text
[ session_id (36) ].[ expires_at_hex (16) ].[ blake3_mac (64) ]
```
- `session_id`: UUID v4 string.
- `expires_at_hex`: Lowercase hexadecimal Unix timestamp (seconds).
- `blake3_mac`: Keyed BLAKE3 MAC of `"session_id.expires_at_hex"` using $K_{\text{server}}$.

In `mac_guard`, the server extracts the string, checks that `expires_at` is in the future, and validates the MAC in under 1 microsecond. If valid, it computes `TokenHash = BLAKE3(token)` as a 32-byte stack array and stores it in request extensions.

### 5.3 Stateless Hashcash Proof-of-Work
To defend `/api/auth/register` and `/api/auth/login` from brute-force attacks without stateful databases:
1. Client requests a challenge via `GET /api/auth/challenge?action=login`.
2. Server returns a challenge string: `[ timestamp_hex (16) ][ mac_hex (64) ][ action (5|8) ]`.
3. Client solves the puzzle by finding a 64-bit nonce such that:
   $$\text{SHA-256}(\text{identity\_hash} \parallel \text{nonce} \parallel \text{challenge}) \text{ starts with 16 zero bits}$$
4. Client sends the result in the `X-Hashcash` header:
   ```text
   [ identity_hash (64) ][ nonce (16) ][ timestamp_hex (16) ][ mac_hex (64) ][ action (5|8) ]
   ```
5. `pow_guard` validates the header in fixed time without allocations: length check, SHA-256 leading bits check, timestamp freshness (<15s), and BLAKE3 MAC check.

### 5.4 Root Envelope Encryption
The true 32-byte master vault encryption key ($K_{\text{vault}}$) is never stored on the server in plaintext. It is wrapped inside a 60-byte ChaCha20-Poly1305 envelope (`wrapped_vault_key`):
- Format: `[ 12-byte Nonce ][ 32-byte Ciphertext ][ 16-byte Poly1305 Tag ]` (120 hex characters).
- Wrapped using $K_{\text{recovery}}$ (derived from the user's 24-word recovery phrase via Argon2id).
- On login and registration, the server stores and delivers this envelope opaquely.
- When regenerating recovery phrases (`PUT /api/v1/auth/recovery`), the client unwraps $K_{\text{vault}}$, re-wraps it under the new recovery key, and uploads the new envelope while atomically revoking old sessions.

### 5.5 WebAuthn Hardware Passkeys and Lossless User Projection
Kosh supports 1-touch passwordless biometric authentication using WebAuthn:
- **Bijective User Handle:** SQLite identifies users via `i64`. WebAuthn Level 3 specifies a 16-byte `user.id` (`Uuid`). Instead of one-way hashing (which requires slow $O(N)$ database scans during discoverable logins), Kosh uses a lossless mathematical projection:
  - `Uuid::from_u128(u128::from(u64::try_from(user_id)?))`
  - `i64::try_from(uuid.as_u128())?`
- **Clone Detection:** On every authentication, the server verifies and updates the authenticator's monotonic signature counter and persists the serialized `webauthn-rs::Passkey` state back to SQLite.
- **Client Metadata Privacy:** Human-readable device labels are client-side encrypted before uploading and restricted to 1024 bytes.

### 5.6 Ephemeral ECDH Blind Mailbox Relay
When a secondary device (e.g., laptop) authenticates via Passkey, it receives `wrapped_vault_key` but lacks $K_{\text{vault}}$ to decrypt it. The devices synchronize keys out-of-band:
1. New device opens a mailbox via `POST /api/v1/devices/mailbox` and displays a QR code with `mailbox_id` and an ephemeral X25519 public key.
2. Existing device scans the QR code, encrypts $K_{\text{vault}}$ with the public key, and deposits the ciphertext via `POST /api/v1/devices/mailbox/{id}/fulfill`.
3. New device polls `GET /api/v1/devices/mailbox/{id}`, picks up the ciphertext, and the server **immediately purges the entry from RAM (burn-after-reading)**.
4. **Guarantees:** Zero disk I/O (Moka RAM cache only, 5-minute TTL, max 1,000 entries, 4 KB max payload) and strict user isolation.

---

## 6. Storage Vault and Transaction Physics

The server storage layer (`crates/server/src/storage/`) enforces atomicity and content-addressability.

### Content-Addressable Storage (CAS)
- Files are named strictly by their 64-character BLAKE3 hex hash: `vault/<blake3_hash>`.
- Path traversal is structurally impossible because filenames are derived entirely from cryptographic digests.

### The Upload Transaction Lifecycle (`storage::Transaction`)
```text
1. Client sends POST /api/v1/upload/{tag}
   Headers: X-File-Name: <uuid.enc>, Content-Length: <bytes>
   Body: Raw byte stream (application/octet-stream)
       │
       ▼
2. Transaction::new() creates temporary staging file:
   Path: vault/<random-uuid>.tmp
       │
       ▼
3. Disk pre-allocation via posix_fallocate (best effort):
   Reserves contiguous sectors on spinning disk
       │
       ▼
4. Incremental streaming & hashing:
   Reads 64 KB buffers from network stream -> writes to .tmp -> feeds blake3::Hasher
       │
       ▼
5. EOF Validation:
   Checks total bytes written == Content-Length header
       │
       ▼
6. Atomic Commit:
   tokio::fs::rename(tmp_path, vault/<blake3_hash>)
       │
       ▼
7. Metadata registration:
   Asset::create() records user ownership in SQLite
```
**Failure Policy:** If the network disconnects, the byte count mismatches, or the disk fills up, the `Transaction` aborts and immediately unlinks the temporary staging file. Zero garbage accumulates in the vault.

---

## 7. Write-Ahead Log (WAL) Ledger Engine

For real-time delta synchronization with mobile clients, the server features an append-only WAL ledger subsystem located at `crates/server/src/storage/ledger/`.

### The Single-Writer Actor Pattern
To avoid complex file locking or mutex contention on disk, all mutating operations for all users are routed to a single `Committer` actor running in a background Tokio task.
- Communication uses a bounded channel: `mpsc::Sender<Action>` (capacity 100).
- Handlers await completion using private `oneshot::channel` reply channels.

### Stateless Zero-Copy Reads
Reading delta segments (`Handle::read_segment`) **bypasses the actor entirely**. The calling HTTP request opens the target file directly with an independent descriptor, validates the path, seeks past the header, and streams data. Thousands of clients can sync concurrently without queueing behind write operations.

### Segment Physics & Rotation
- Storage layout:
  ```text
  vault/ledgers/<user_id>/
  ├── CURRENT          # Atomic pointer file containing the active segment filename
  ├── delta_0000001    # 500-byte KOSH header + appended action frames
  └── delta_0000002
  ```
- **KOSH Header (500 Bytes):** Offset 0–3: `b"KOSH"`; Offset 4–5: version `1u16`; Offset 6–9: previous segment ID `u32`. All read operations clamp seek offsets to at least 500 bytes to ensure the header is never exposed over the wire.
- **Payload Framing:** The API layer prepends each payload with a 4-byte little-endian length prefix `[4-byte LE len][payload]`. The server treats payload bytes as completely opaque.
- **Rotation Boundary:** When a segment exceeds ~5 MB (5,000,000 bytes), the next append rotates to a new segment file (`delta_0000002`) and atomically updates the `CURRENT` pointer via a `.tmp` file rename.
- **Crash Safety (Ghost Files):** If the server crashes mid-rotation, `Segment::create` unlinks any leftover partial file before opening with `create_new(true)`.

---

## 8. Database and Keyset Cursor Pagination

Kosh uses SQLite with WAL (Write-Ahead Logging) enabled.

### Database Schema
1. **`users`:** Stores `id` (INTEGER PRIMARY KEY), `identity_hash` (BLOB, unique), `auth_verifier` (TEXT), and `wrapped_vault_key` (BLOB, exactly 60 bytes).
2. **`sessions`:** Stores `session_id` (BLOB), `user_id` (INTEGER), and expiration timestamps.
3. **`assets`:** Stores user ownership over CAS blobs with tags.
4. **`webauthn_credentials`:** Stores `credential_id` (BLOB, PK), `user_id` (FK to users), `passkey_data` (BLOB), `counter` (INTEGER), `encrypted_metadata` (BLOB, max 1024 bytes), `created_at`, `last_used`.

### Keyset Indexing Strategy
To support listing millions of assets on spinning disks without degradation, we maintain a composite B-Tree index:
```sql
CREATE INDEX idx_assets_user_tag_cursor
ON assets(user_id, tag, last_modified DESC, id DESC);
```
- **Keyset Seek:** `WHERE user_id = ? AND tag = ? AND (? IS NULL OR last_modified < ? OR (last_modified = ? AND id < ?))`
- **Sentinel Fetch (`limit + 1`):** Queries request one extra item to determine if `next_cursor` exists, avoiding `COUNT(*)` full-table scans.

---

## 9. Zero-Heap Error Architecture

All error types in the server implement zero-allocation design principles:
1. **`&'static str` Error Messages:** Variants like `BadRequest(&'static str)` and `NotFound(&'static str)` use compile-time static strings instead of heap-allocated `String`s.
2. **`wrap_internal_err!` Macro:** Located in `crates/server/src/error/internal.rs`, this macro generates zero-overhead `From` implementations bridging standard library errors (`TryFromIntError`, `SystemTimeError`) through `internal::Error` directly to domain error enums.
3. **`Loggable` Trait:** Every error domain implements `Loggable`, returning `Level` and `Module` without string formatting.
4. **Sanitized HTTP Responses:** Internal database and filesystem error details are masked behind generic status codes (`500 Internal Server Error`) to prevent internal structure leakage.

---

## 10. Telemetry and The Ratatui Admin CLI

The telemetry pipeline runs entirely asynchronously:
1. `crate::Error::into_response()` inserts an `Entry` into response extensions.
2. `log_middleware` forwards the `Entry` to `GLOBAL_LOGGER.try_send(entry)`.
3. Background `LoggerService` writes the bincode-encoded `Entry` to daily `.bin` log files under `vault/state/logs/`.
4. Concurrently, it broadcasts `Telemetry::Log` frames over a Unix Domain Datagram Socket (`/tmp/kosh.sock`).
5. The `cli` crate (`crates/cli`) connects to `/tmp/kosh.sock` and renders a live, interactive Ratatui dashboard with level filtering, keyword search, and device pairing QR codes.

---

## 11. Development Setup and Workflow

### Prerequisites
- Rust 1.80+ (Stable toolchain)
- SQLite 3
- `sqlx-cli` (`cargo install sqlx-cli --no-default-features --features sqlite`)

### Building and Running
```bash
# Build the entire workspace
cargo build --workspace

# Run the server daemon
cargo run -p webdav-server

# Run the administrative TUI monitor
cargo run -p cli
```

### Trunk-Based Development (TBD)
We follow Trunk-Based Development:
1. All work happens on short-lived feature or documentation branches branched off the latest `main`.
2. Keep branches focused and short-lived (merge within 1-2 days).
3. Use Conventional Commits (`feat(server): ...`, `fix(model): ...`, `docs(...)`, `test(...)`).
4. Pull Requests must pass all 5 automated GitHub Actions checks and receive 1 approving peer review before merging.

### Preparing SQLx Offline Cache
Kosh uses SQLx compile-time query verification with an offline cache (`.sqlx/`). If you alter database queries or add migrations:
```bash
cargo sqlx prepare --workspace
```

---

## 12. Coding Standards and Lint Contract

### Workspace Lint Contract
The repository enforces strict workspace-level Clippy lints in `Cargo.toml`:
- `unwrap_used = "deny"` — No `.unwrap()` in production server code. Handle errors with `?` or explicit matching.
- `expect_used = "deny"` — No `.expect()`.
- `indexing_slicing = "deny"` — No raw array indexing that could panic (`arr[i]`). Use `.get()`.
- `todo = "deny"` — No unfinished `todo!()` macros in committed code.
- `panic = "deny"` — No explicit panics.
- `as_conversions = "deny"` — No raw `as` numeric casts. Use `try_from` or `from`.

### Local Verification Commands
Before submitting a pull request, run the complete verification suite locally:

```bash
# 1. Check formatting
cargo fmt --all -- --check

# 2. Check strict clippy lints across all targets
cargo clippy --workspace --all-targets -- -D warnings

# 3. Run all workspace unit and integration tests
cargo test --workspace

# 4. Verify documentation compiles with zero warnings or broken links
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Thank you for helping build Kosh!
