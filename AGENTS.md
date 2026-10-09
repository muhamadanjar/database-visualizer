# Repository Guidelines

## Project Structure & Module Organization

`PRD.md` defines Database Studio, a PostgreSQL/MySQL desktop and web client. Documentation lives in `docs/`; shared Rust Foundation is implemented. Desktop/web applications remain planned.

Layout: `crates/` for implemented Rust libraries/adapters; planned `apps/desktop/` for Tauri, `apps/web/` for React, `services/api/` for Axum, `packages/` for shared frontend components/types, and `infra/docker/` for development databases. Phase 1 starts with `db-types`, `db-core`, `db-postgres`, and `db-mysql`; metadata/query lifecycle are modules initially. Complete Foundation acceptance criteria before desktop work.

## Build, Test, and Development Commands

Rust commands and Compose are available; setup is in `README.md`:

- `docker compose up -d --wait`: disposable development databases.
- `cargo build --workspace --locked`: build the core and adapters.
- `cargo run -p db-core --locked --example foundation -- postgres` (or `mysql`): smoke example.

Use `cargo fmt --check`, `cargo clippy --workspace --all-targets`, and `cargo test --workspace --locked`; add `-- --include-ignored` for Docker integration tests. No frontend dev script exists yet.

## Coding Style & Naming Conventions

Use stable Rust, explicit types, typed errors, rustfmt/four-space indentation, `snake_case` functions/modules, and `PascalCase` types. Avoid production `unwrap()`. Keep core independent of UI/network frameworks; engine-specific behavior belongs in adapters. For TypeScript, use two spaces, `PascalCase` components, and `camelCase` functions. Formatting and Clippy must pass without warnings.

## Testing Guidelines

Test capabilities, metadata, lifecycle, and errors. Integration tests in `crates/db-core/tests/` verify both engines using disposable Docker databases, including session persistence, bounded streaming, and actual cancellation. Name tests by behavior. Frontend/E2E frameworks and coverage thresholds remain undecided.

## Commit & Pull Request Guidelines

No Git history establishes conventions. Use concise imperative subjects. PRs describe behavior, PRD references, validation, remaining gaps, and architectural deviations; include screenshots for UI changes.

## Documentation Workflow

Use Indonesian and maintain [docs/README.md](docs/README.md) plus directory indexes:

- `docs/features/<slug>.md`: purpose, behavior, scope, acceptance, and status.
- `docs/plans/phase-<number>-<slug>.md`: dependencies, checklists, blockers, and validation.
- `docs/design-docs/YYYY-MM-DD-<slug>.md`: decisions, alternatives, risks, and open questions; preserve `TEMPLATE.md` sections, using justified `N/A`.

Link related documents and update them with behavior/contract changes. Confirm shared understanding before writing designs; require human review/approval before implementing architecture, schema, public API, cross-service, or risky operational changes. Unknowns use `[perlu konfirmasi: ...]`; resolve blockers before dependent work.

Design status: `Draft → In Review → Approved → Implemented`; `Rejected`/`Deprecated` are terminal. `In Review` requires human review. Plans use `Belum dimulai`, `Berjalan`, `Terblokir`, `Selesai`. Mark `[x]` only with dated artifact, validation steps/results, and limitations. Distinguish static, unit, integration, browser/E2E, benchmark, and pipeline evidence. Phase 5 remains future scope.

## Security & Configuration

Never commit/log credentials. Desktop secrets use OS credential storage; web secrets remain backend-managed and encrypted. Enforce tenant authorization and separate customer SQL from application PostgreSQL.
