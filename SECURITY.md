# Security Policy

Kosh is an end-to-end encrypted, zero-knowledge storage vault. This document defines our threat model scope and the private vulnerability reporting process.

---

## Threat Model and Scope

### In-Scope Vulnerabilities

- **Cryptographic Invariants:** Flaws in BLAKE3 MAC generation/verification, WebAuthn assertion validation or counter tracking, ChaCha20-Poly1305 envelope encryption, or X25519 ECDH blind mailbox mechanics.
- **Tenant Isolation:** Cross-user data leakage, unauthorized segment reads, or bypassing `mac_guard` / `auth_guard` session resolution.
- **Ingress and Denial of Service:** Circumvention of `tower_governor` IP shields, Hashcash Proof-of-Work verification bypasses, or unbounded memory allocations leading to async worker starvation.
- **Storage and Filesystem Integrity:** Path traversal outside `vault/`, tampering with two-level prefix sharding, or corrupting append-only WAL ledger segments.
- **Database Security:** SQL injection or unauthorized mutations of SQLite tables (`users`, `sessions`, `assets`, `webauthn_credentials`).

### Out-of-Scope Vulnerabilities

- Physical attacks requiring direct physical access to the host machine without full-disk encryption.
- Attacks requiring root or superuser execution privileges on the host OS.
- Attacks on client device hardware keystores or compromised mobile operating systems.
- Network-level volumetric DoS exceeding upstream line rate.

---

## Reporting a Vulnerability

**Do not file public GitHub issues for security vulnerabilities.**

### Primary Channel: GitHub Security Advisories

Submit a confidential report through GitHub Security Advisories:
[Submit Advisory](https://github.com/kosh-project/server/security/advisories/new)

Reports submitted here remain private and encrypted between the reporter and maintainers.

### Secondary Channel: Direct Maintainer Contact

If GitHub Security Advisories are inaccessible, email the maintainer:
- **Maintainer:** Nilesh Kumar (`@thenil3sh`)
- **Email:** `107793551+thenil3sh@users.noreply.github.com`

---

## What to Include

To accelerate triage, include:

1. A clear description of the vulnerability and its potential impact.
2. Step-by-step reproduction instructions or a minimal Proof of Concept (PoC) script.
3. The affected crate (`server`, `kosh-core`, `cli`), commit hash, and environment details.
4. Any proposed patches or mitigations.

---

## Response Timeline

- **Triage:** Acknowledgment of receipt within 48 hours.
- **Assessment:** Validation and severity assessment within 5 business days.
- **Remediation:** Coordinated patch development and verification in a private branch.
- **Disclosure:** Coordinated public release and advisory publication after downstream systems can update.
