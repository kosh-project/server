# Security Policy

## Reporting Security Vulnerabilities

Kosh is designed from the ground up as a zero-knowledge personal storage engine. We take cryptographic security, tenant isolation, and private key protection with the utmost seriousness.

If you believe you have discovered a vulnerability related to:
- Cryptographic implementations (BLAKE3 MAC verification, WebAuthn assertions, root envelope encryption, ECDH key exchange)
- Cross-tenant data leakage or bypass of authenticated ingress guards
- Memory corruption, denial-of-service, or unmetered resource exhaustion
- File path traversal or storage vault integrity bypass

**Please do not open a public GitHub issue.**

### How to Report Privately

1. **GitHub Security Advisory (Recommended):** Submit a confidential vulnerability report directly through GitHub at [kosh-project/server/security/advisories/new](https://github.com/kosh-project/server/security/advisories/new).
2. **Direct Contact:** If GitHub Security Advisories are unavailable, contact the project maintainers directly via private channels.

### What to Include in Your Report
To help us triage and resolve the issue quickly, please provide:
- A clear description of the vulnerability.
- Step-by-step reproduction instructions or a minimal Proof of Concept (PoC).
- The affected crate, version, or commit hash.
- Any suggested mitigations or patches, if available.

### Our Commitment
- We will acknowledge receipt of your vulnerability report within 48 hours.
- We will provide regular status updates while working on a patch.
- We will coordinate public disclosure with you after a fix has been verified and released.
