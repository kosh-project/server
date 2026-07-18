# server

A self-hosted personal storage platform — the Rust backend for a zero-knowledge photo and file management system designed for home-lab deployment.

---

## The Vision

The goal is a self-hosted alternative to Google Photos and Google Drive that a non-technical user can run on old hardware at home, where the server operator learns nothing about what is stored on it. Photos, videos, and files are encrypted on the Android client before transmission. The server stores only ciphertext and metadata — it is architecturally incapable of reading what it holds.

This is not a file sync daemon or an SMB server. Think closer to Immich or Synology Photos in terms of user experience, but built from scratch around a zero-knowledge constraint.

The project is also a deliberate learning exercise in systems programming. Architecture decisions are made to understand the underlying concepts, not just to ship.

---

## What Is Currently Built

The server-side foundation is functional:

- **Streaming upload pipeline** with atomic BLAKE3 Content-Addressable Storage
- **User registration and login** with no plaintext password storage
- **Session token authentication** with inline expiry handling
- **Auth middleware** that guards all protected routes before any handler executes
- **Asset metadata schema** wired to the upload pipeline (in progress)
- **SQLite migrations** managed via `sqlx`

The Android client does not exist yet. The server has no stable public API.

---

## Architecture

### Layers

```
Android Client  ←  encrypts everything before it leaves the device
      |
      | HTTPS  (certificate-pinned, planned)
      v
  API Layer     ←  Axum handlers, auth middleware, multipart parsing
      |
  Storage Layer ←  streaming writes, atomic CAS, BLAKE3
      |
  SQLite        ←  metadata index (users, sessions, assets)
  Filesystem    ←  encrypted blobs named by their BLAKE3 hash
```

### Content-Addressable Storage

Files on disk are named by their BLAKE3 hash, not by anything the client provides. The upload flow:

1. Incoming bytes stream to a UUID-named `.tmp` file — the client-provided filename is never used as a path component, making path traversal impossible by design.
2. BLAKE3 is computed incrementally as chunks arrive from the network.
3. On completion, the temp file is atomically renamed to the hash via a single `rename(2)` syscall. On Linux this is guaranteed atomic by POSIX.
4. On any failure, the temp file is deleted immediately. No partial files accumulate.

Two identical files from two different users occupy one slot on disk. Deduplication is a structural property, not a feature.

### Authentication

Opaque session tokens rather than JWTs. The reason: JWTs are not revocable without a server-side denylist, which makes them effectively the same thing as opaque tokens but with worse security properties. Here we generate a UUID v4 token, BLAKE3-hash it, store the hash in SQLite, and return the plaintext token to the client exactly once. The server never stores the plaintext.

On every protected request, `auth_guard` middleware extracts the `Authorization: Bearer` header, hashes the token, looks it up in the `sessions` table, checks expiry, and injects the `user_id` into request extensions. Handlers downstream just extract `Extension<i64>`.

### Route Structure

```
/health              — unauthenticated health check
/api/auth/register   — public
/api/auth/login      — public
/api/v1/upload       — protected (auth_guard applied at router level)
/api/v1/storage      — protected
```

The auth middleware is applied via `.route_layer()` on the `v1` router, not checked inside individual handlers. New protected endpoints automatically inherit it.

### Upload Protocol

Uploads use `multipart/form-data` with a strict field ordering requirement:

1. `tag` field must arrive first — an integer (`0`–`3`) identifying the asset type
2. `file` field follows — encrypted bytes with a `filename` attribute

The server fails immediately (HTTP 400) on any protocol violation. There is no fallback behavior for missing filenames or missing tags. The rationale: silent fallbacks hide client bugs; hard failures surface them immediately during development.

Asset tags are stored as integers in SQLite:

| Value | Meaning |
|---|---|
| 0 | GalleryMeta |
| 1 | GalleryItem |
| 2 | DriveMeta |
| 3 | DriveItem |

Batch uploads return a per-file JSON report card. A single file failure does not abort the batch.

---

## Security Design

> [!NOTE]
> The zero-knowledge architecture is the long-term target. Client-side encryption and certificate pinning are planned but not yet implemented. What IS implemented is the server-side constraint: the upload pipeline treats all bytes as opaque ciphertext and enforces that the server never inspects content.

### Zero-Knowledge Constraint

The server does not sniff magic bytes, infer MIME types, parse filenames, or inspect content. This is enforced architecturally: the storage layer receives a byte stream and a hash — nothing else. All semantic metadata (what the file actually is, its original name, EXIF data) lives in an encrypted index on the client.

### Certificate Pinning — Out-of-Band Pairing (Planned)

On first boot, the server will generate a self-signed X.509 certificate and display its SHA-256 fingerprint as a QR code in the terminal. The Android client scans this out-of-band and pins the certificate via a custom `X509TrustManager`. Any future connection presenting a different certificate is immediately terminated.

This eliminates the need for a certificate authority while providing strong MitM protection — the fingerprint is shared physically, not over the network.

---

## Offline-First Sync (Planned)

The sync architecture uses event sourcing to avoid the lost-update problem when multiple devices modify their state while offline:

- Devices upload small encrypted action events (`MOVE_FILE`, `DELETE_FILE`, etc.) to a server-side ledger
- The server stores them sequentially without understanding them
- Clients pull the ledger and replay events to reconstruct local state deterministically
- Periodic encrypted `metadata.db` snapshots let new devices fast-forward without replaying the full history

The server retains the last three snapshots for recovery from corruption.

---

## Stack

| | |
|---|---|
| Language | Rust |
| HTTP framework | Axum + Tokio |
| Database | SQLite via `sqlx` (compile-time verified queries) |
| Hashing | BLAKE3 |
| Migrations | `sqlx migrate` |
| Target OS | Arch Linux (headless, `linux-lts` kernel) |
| Target hardware | Legacy x86_64, ~10 GB RAM |

Compiled with `RUSTFLAGS="-C target-cpu=native"` to use AVX/SSE4.2 for BLAKE3 throughput on the target machine.

---

## Getting Started

```bash
# Install sqlx-cli if you don't have it
cargo install sqlx-cli --no-default-features --features sqlite

# Run migrations
sqlx migrate run --database-url sqlite://test/vault/metadata.db

# Start the server
cargo run
```

Server listens on `0.0.0.0:6969`. Vault and database live under `test/vault/` during development.

```bash
# Run tests
cargo test
```

Integration tests use an in-memory SQLite database and temporary directories. They don't touch the development vault.

---

## Where Things Are Headed

The immediate next milestones, roughly in order:

1. Finalize the upload pipeline — `Asset::create` wired end-to-end
2. In-memory session cache with `moka` for near-zero auth overhead
3. Gallery listing API (`GET /api/v1/assets`)
4. Certificate generation and QR pairing on startup
5. Android client — the part that makes any of this usable


# ToDo
- [ ] Upload transaction
- [ ] Dang it bro
- [ ] More refactoring
- [ ] Handle Directory Traversal
- [ ] Separate `storage manager` layer from `api` layer;
- [ ] `(NEEDS FIX)` Multipart Error's are ignored on chunk iteration
- [ ] Invalidate requests with directory traversal
- [ ] `(new)` TaskRegistry, 
- [ ] 
```rs
pub trait ReportProgress
```


# Features 
- [ ] Resumable Uploads
- [ ] Garbage collector
- [ ] Multiple user sessions
- [ ] Google Photos/ Drive migration
- [ ] Resumable uploads
