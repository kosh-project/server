## Summary

A concise summary of the changes introduced in this pull request.

## Problem and Context

What problem does this change solve? If related to an open issue, reference it here (e.g. `Fixes #123`).

## Type of Change

- [ ] Bug fix (non-breaking fix)
- [ ] New feature (non-breaking addition)
- [ ] Breaking change (modifies existing API or behavior)
- [ ] Refactor (internal restructuring with zero behavioral change)
- [ ] Documentation update
- [ ] Performance optimization (disk I/O, memory allocations, or CPU efficiency)

## Verification Checklist

Ensure the following local checks pass before requesting review:

- [ ] Code formatted: `cargo fmt --all -- --check`
- [ ] Strict Clippy lints pass: `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Workspace tests pass: `cargo test --workspace`
- [ ] Documentation compiles without warnings: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Added test coverage for new or modified behavior
- [ ] If SQL queries or migrations changed: offline cache refreshed via `cargo sqlx prepare --workspace`
- [ ] Zero-knowledge and low-resource invariants maintained (no unmetered heap allocations, zero random disk seeks on error paths)
- [ ] No emojis introduced in code, documentation, or commit messages
