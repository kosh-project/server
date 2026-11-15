# Support Guide

This guide explains how to get help with Kosh server deployments, architecture questions, and bug investigations.

---

## Channels

### GitHub Discussions

Use [GitHub Discussions](https://github.com/kosh-project/server/discussions) for:

- Hardware recommendations and tuning for low-power deployments (Sandy Bridge, ARM SBCs, mechanical HDDs)
- Architecture questions regarding zero-knowledge trust boundaries, WebAuthn passkeys, or WAL ledger sync
- KDL configuration questions (`config.kdl`)
- General community discussions

### GitHub Issues

Use [GitHub Issues](https://github.com/kosh-project/server/issues) strictly for:

- Reproducible bug reports with terminal logs and environment specifications
- Well-specified feature proposals evaluating low-resource hardware impact

Before filing an issue, search existing open and closed issues to avoid duplicates.

### Security Vulnerabilities

Do not file public issues for security vulnerabilities. See [SECURITY.md](SECURITY.md) for confidential reporting channels via GitHub Security Advisories.

---

## Diagnostic Information

When reporting an issue or asking for deployment assistance, collect the following systems-level diagnostic information:

### 1. Host Hardware and Memory
```bash
# Kernel version and architecture
uname -a

# Available physical memory and swap
free -h
```

### 2. Disk IO and Storage
```bash
# Check filesystem mount options and free space
df -hT ./vault

# Monitor disk latency and queue depth under load
iostat -xz 1 5
```

### 3. Server Version and Build Profile
```bash
# Active Rust toolchain
rustc --version
cargo --version

# Built commit
git rev-parse HEAD
```

### 4. Telemetry and Logs
- **Live Telemetry:** Run `cargo run -p cli` to attach to `/tmp/kosh.sock` and observe real-time error frames.
- **Log Files:** Daily structured logs are written in binary format to `vault/state/logs/`.

---

## Scope

This repository maintains the backend server daemon (`server`), shared core library (`kosh-core`), and the administrative TUI (`cli`). Issues pertaining to mobile clients (such as Android Keystore integration or WorkManager sync) belong in the respective client repository.
