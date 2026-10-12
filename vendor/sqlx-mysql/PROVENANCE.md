# sqlx-mysql 0.8.6

Source: crates.io release; package SHA256 `aa003f0038df784eb8fecbbac13affe3da23b45194bd57dba231c8f48199c526`.
Phase 2 source modification: read-only protocol status accessor in `src/connection/mod.rs`.
Approved by user 2026-10-09; see docs/design-docs/phase-2-driver-status-review.md.

Phase 3 adds an optional validated TCP destination in `src/options/mod.rs` and connection establishment; the original hostname remains the TLS identity. Approved separately by user 2026-10-10. Exact diff: `docs/design-docs/phase-3-destination-pinning.patch`; review and runtime evidence limits: `docs/design-docs/phase-3-driver-pinning-review.md`. No executor/decoding changes.
