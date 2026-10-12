# Repository Guidelines

## Project Structure & Module Organization

`crates/db-{types,core,postgres,mysql}` contain contracts/adapters; `crates/desktop-runtime` owns sessions/storage/results. `apps/desktop` contains React/Vite and Tauri. `packages/ui` and `packages/shared-types` provide reusable components and transport contracts. `crates/query-runtime` owns shared script/spool, bounded search, and atomic editing logic. `apps/web` and `services/api` implement the Web MVP. Fixtures: `infra/docker`, `compose.yaml`.

Prefer codebase-memory MCP; fallback for configuration/insufficient results.

## Build, Test, and Development Commands

- `pnpm install --frozen-lockfile`: install frontend dependencies.
- `pnpm desktop`: run Tauri; `pnpm dev` previews UI without database access.
- `pnpm build`: desktop frontend; `pnpm build:web` and `pnpm web`: web build/development.
- `cargo run -p studio-api --locked`: API; follow the web runbook for keys, policy, and separate migration.
- `docker compose up -d --wait`: start disposable databases.
- `cargo test -p db-core -p db-types -p desktop-runtime -p query-runtime -p studio-api --features studio-api/test-provider --locked -- --include-ignored`: core/security and database tests.
- `pnpm test`: Vitest/React Testing Library; `pnpm test:native`: isolated automation build and native WebdriverIO tests.

Use `cargo fmt --all --check`, workspace Clippy with `-D warnings`, `pnpm typecheck`, `pnpm typecheck:web`, and `pnpm format:check`. See `README.md`.

## Coding Style & Testing

Use rustfmt, typed errors, `snake_case` functions, and `PascalCase` types; avoid production `unwrap()`. Keep core independent of UI, transports, and concrete adapters. TypeScript: strict checking, Prettier, two spaces, `PascalCase` components, `camelCase` functions. Test failures, cancellation, precision, and storage. Use disposable databases; native tests require a desktop.

## Commit & Pull Request Guidelines

Use concise imperative commit subjects. Describe behavior, PRD references, validation, limitations, and architecture. Include UI screenshots. Preserve the approved minimal SQLx patch and provenance; re-review upgrades. Separate upstream warnings from workspace Clippy failures.

## Documentation & Security

Maintain Indonesian documentation and indexes:

- `docs/features/<slug>.md`: behavior and acceptance.
- `docs/plans/phase-<number>-<slug>.md`: checklists and evidence.
- `docs/design-docs/YYYY-MM-DD-<slug>.md`: decisions; preserve `TEMPLATE.md` sections.

Require human approval for architecture, schema, public API, or risky operational changes; existing session authorization remains valid. Designs use `Draft → In Review → Approved → Implemented`; plans use `Belum dimulai`, `Berjalan`, `Terblokir`, `Selesai`. Check `[x]` only with dated evidence and limits; separate unit, database, browser/native, benchmark, and pipeline proof. Phase 5 remains future scope.

Disable `test-provider` in production; fixture login is not OAuth proof. Follow Phase 3 operations and Phase 4 validation/contracts. Schema v2 requires backup and separate migration. Never replay Unknown mutations; keep row drafts out of persistence. Never log/commit secrets. Use OS credential storage/memory-only connections, encrypt application-persisted SQL, preserve privacy, and separate customer SQL from application PostgreSQL.
