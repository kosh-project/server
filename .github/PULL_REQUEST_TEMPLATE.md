## Description
<!-- Provide a brief description of what this PR introduces, fixes, or refactors. -->

## Motivation & Context
<!-- Why is this change required? What problem does it solve? -->
<!-- If it fixes an open issue, link it here: Fixes #<issue_number> -->

## Type of Change
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Refactor / Code cleanliness (no functional change)
- [ ] Documentation update
- [ ] Performance optimization

## Verification Checklist
Before submitting, please ensure your changes pass our local engineering standards:
- [ ] Code formatted with `cargo fmt --all -- --check`
- [ ] Strict Clippy checks pass: `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] All unit and integration tests pass: `cargo test --workspace`
- [ ] Documentation builds with zero warnings: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`
- [ ] Added unit or integration tests for new functionality
- [ ] If SQL queries or migrations were modified, updated SQLx cache with `cargo sqlx prepare --workspace`
- [ ] Preserved zero-knowledge and low-resource invariants (no unmetered memory allocations, zero disk seeks in error paths)
