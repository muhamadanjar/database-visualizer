# Phase 1 — Foundation

**Status:** Selesai  
**Sumber:** PRD §§6–12, 17, 19–20, 34–39, 41, 45–48, 54–57.  
**Desain:** [Foundation — Implemented](../design-docs/2026-10-09-foundation.md).  
**Fitur:** [Core & adapter](../features/database-core-adapters.md), [koneksi](../features/connection-security.md), [metadata](../features/database-explorer-inspector.md), [query & session](../features/query-sessions-streaming.md).

## Tujuan & batasan

Bangun shared Rust core yang bisa menghubungkan PostgreSQL dan MySQL, membaca metadata, mengeksekusi serta melakukan streaming query, dan menghentikan query di database. Tidak mencakup React, Tauri, Axum, autentikasi web, deployment produksi, atau implementasi Phase 2–5.

## Prasyarat & keputusan sebelum implementasi

- [x] Pengguna mereview desain melalui Q1–Q20 dan mengotorisasi implementasi Phase 1.
- [x] Kontrak types/IDs/values/events/errors tersedia di `crates/db-types/src/lib.rs`; signature adapter/sink/session tersedia di `crates/db-core/src/lib.rs`.
- [x] Limit session, batch, queue, consumer/cleanup timeout, serta admission policy ditetapkan dalam `RuntimeLimits`; busy session ditolak.
- [x] Cancellation/control/recovery contract diputuskan dan diuji lewat spike serta integration/policy tests; tidak ada otomatis rollback/replay.
- [x] Rust 1.99.0, SQLx 0.8.6, sqlparser 0.63.0, dependency lockfile, dan Compose image digests dikunci.
- [x] Identifier metadata terstruktur; introspection memakai parameter binding dan scope engine/database/schema.
- [x] Spike driver/parser lulus sebelum adapter/core dilengkapi; batas DO/CALL/script dan MySQL SLEEP dicatat pada desain/README.

Default resource belum merupakan hasil benchmark atau batas total RSS. Identitas author/reviewer belum diberikan; reviewer/otorisasi berasal dari pengguna dalam sesi ini.

## Milestone A — Workspace & environment

- [x] Inisialisasi monorepo dan Rust workspace dengan `db-types`, `db-core`, `db-postgres`, serta `db-mysql`.
- [x] Jaga dependency core terhadap abstraksi/types; hubungkan adapter konkret di host/test, tanpa dependency pada UI/network framework.
- [x] Tempatkan metadata dan query lifecycle sebagai modul dalam empat crate; dokumentasikan jika pemisahan tambahan diperlukan.
- [x] Sediakan Docker Compose untuk PostgreSQL customer, MySQL customer, dan PostgreSQL aplikasi yang terpisah.
- [x] Dokumentasikan konfigurasi development tanpa credential nyata; pastikan fixture/test tidak memakai database produksi.
- [x] Tambahkan README berisi arsitektur, setup, konfigurasi, build, unit test, dan integration test.
- [x] Dokumentasikan executable target sebelum menawarkan `cargo run`; jangan mengklaim `pnpm dev` tersedia sebelum frontend dibuat.

## Milestone B — Types & kontrak core

- [x] Implementasikan tipe konfigurasi koneksi, engine, capabilities, identifier connection/session/query, dan database object.
- [x] Definisikan trait adapter async dan kontrak connect, introspect, execute, cancel, serta disconnect/cleanup.
- [x] Definisikan query lifecycle `Created`, `Running`, `Streaming`, `Completed`, `Failed`, dan `Cancelled` beserta transisi valid.
- [x] Definisikan event start, columns, rows, completion, failure, cancellation dan metadata eksekusi; finalisasi serialisasi sebelum transport dibuat.
- [x] Implementasikan typed errors dan pemetaan kategori tanpa credential atau stack trace internal dalam pesan publik.
- [x] Implementasikan session dengan satu koneksi tetap dan maksimal satu query aktif; query paralel memakai session berbeda.
- [x] Terapkan kebijakan bounded resource dan cleanup yang telah disepakati.

## Milestone C — Adapter & metadata

- [x] PostgreSQL: connect, test connection, disconnect, reconnect, dan capability detection.
- [x] MySQL: connect, test connection, disconnect, reconnect, dan capability detection.
- [x] PostgreSQL: list databases/schemas, tables, views, dan sequences.
- [x] MySQL: list databases, tables, dan views; jangan mengarang dukungan schema/sequence seperti PostgreSQL.
- [x] Kedua engine: inspect columns, data types, nullable/default, primary key, foreign keys, unique constraints, indexes, dan constraints.
- [x] Metadata bisa diminta per scope/object tanpa mengambil seluruh katalog pada connect.
- [x] Mapping metadata mempertahankan detail engine melalui capabilities dan typed data.
- [x] Catat cakupan metadata Foundation dan bedakan dari object support lengkap MVP pada PRD §10.

## Milestone D — Query, streaming & cancellation

- [x] Eksekusi SQL pengguna pada connection/session yang dipilih, tanpa otomatis mengubah SQL.
- [x] Pastikan query tidak dapat diarahkan ke PostgreSQL aplikasi/control plane.
- [x] Stream columns dan rows dalam batch sebelum seluruh hasil terkumpul; pertahankan urutan event.
- [x] Terapkan bounded buffer dan kebijakan consumer lambat/terputus; hasil tidak ditampung tanpa batas.
- [x] Sertakan waktu mulai, status, durasi, affected rows/rows returned sesuai kontrak final.
- [x] Cancellation benar-benar meminta engine menghentikan query, termasuk typed error jika gagal.
- [x] Tangani error, disconnect, completion/cancel race, dan cleanup sesuai kontrak final.
- [x] Buktikan transaksi, pengaturan session, dan temporary table tetap memakai koneksi yang sama.

## Milestone E — Validasi & gate Phase 2

- [x] Unit/policy tests mencakup capabilities, typed values, lifecycle/terminal state, session admission, dan error/cleanup behavior; metadata mapping dibuktikan integration tests.
- [x] Integration tests PostgreSQL memverifikasi seluruh workflow pada tabel acceptance di bawah.
- [x] Integration tests MySQL memverifikasi seluruh workflow pada tabel acceptance di bawah.
- [x] Pengujian menunjukkan batch dapat dikonsumsi sebelum query selesai dan buffering mengikuti batas yang disepakati.
- [x] Pengujian menunjukkan query panjang berhenti di database setelah cancel, beserta kondisi session sesudahnya.
- [x] Pengujian menunjukkan query kedua pada session sibuk mengikuti kontrak yang disepakati; session berbeda dapat berjalan paralel.
- [x] Pengujian menunjukkan disconnect/cleanup melepas resource dan error tidak membocorkan secret.
- [x] Formatting, lint, unit test, dan integration test berhasil dengan perintah yang didokumentasikan.
- [x] Catat bukti, batas pengujian, dan penghambat; Phase 1 baru `Selesai` jika seluruh acceptance wajib lulus.

## Acceptance criteria

| Workflow | PostgreSQL | MySQL |
|---|---|---|
| Connect, execute query, stream rows, cancel running query, disconnect | Wajib | Wajib |
| List databases | Wajib | Wajib |
| List schemas | Wajib | N/A: gunakan model database MySQL |
| List tables dan views | Wajib | Wajib |
| List sequences | Wajib | N/A: tidak dipaksakan |
| Inspect columns, indexes, constraints | Wajib | Wajib |

Gate tambahan: core independen dari UI/transport; session persistence terbukti; test menggunakan database Docker disposable. Object metadata lengkap MVP tetap menjadi gate Desktop MVP.

## Validasi & progres

Validasi lokal pada 2026-10-09; artefak implementasi: [workspace](../../Cargo.toml), [contracts](../../crates/db-types/src/lib.rs), [core](../../crates/db-core/src/runtime.rs), [PostgreSQL](../../crates/db-postgres/src/lib.rs), [MySQL](../../crates/db-mysql/src/lib.rs), [Compose](../../compose.yaml).

| Bukti | Langkah / artefak | Hasil |
|---|---|---|
| Driver/parser spike | `crates/db-core/tests/spike.rs` | Session/temp table, non-superuser cancel/recovery, parser semicolons/CALL/DO, binary catalog names lulus |
| Unit/policy | `db-types` unit tests, `db-core` SQL validation, `tests/runtime.rs` | Engine capabilities, lossless values, pre-start cancellation, denied cancel, quota release, cleanup deadline, query timeout, host drop lulus |
| Database integration | `crates/db-core/tests/foundation.rs` | Workflow metadata/PK/FK/unique/check/index, session transaction, both engines, streaming/backpressure/large row, cancel/blocked lock, reconnect/drop, typed values, completion race lulus |
| Seluruh suite | `cargo test --workspace --locked -- --include-ignored` | 27 tests lulus; 16 memakai database Docker, 11 unit/policy/parser/config |
| Formatting | `cargo fmt --all --check` | Lulus |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lulus |
| Build | `cargo build --workspace --locked` | Lulus |
| Smoke example | `cargo run -p db-core --locked --example foundation -- postgres` dan `mysql` | Metadata count dan SELECT streamed sampai QueryCompleted |
| Development environment | `docker compose up -d --wait`, `docker compose config --quiet` | Tiga service sehat; PostgreSQL 16.13, MySQL 8.4.8; application database terpisah |

Keterbatasan: tidak ada klaim hard RSS ceiling, benchmark target desktop, UI/native/browser/E2E, production TLS/deployment, semua tipe/dialect, atau pipeline. Privilege rejection/cleanup timeout memakai adapter terkontrol; cancellation akun biasa dibuktikan pada database nyata. Guard control-plane menolak endpoint yang didaftarkan host, bukan otorisasi DNS-aware atau tenant security lengkap.

Penyimpangan teknis yang terdokumentasi: DO belum dipahami parser; CALL, PREPARE/EXECUTE, COPY dan executable comments ditolak sebelum eksekusi untuk menjaga subset satu statement/hasil Foundation. MySQL SLEEP dapat melaporkan sukses setelah interruption; completion tetap menang dan `results_complete=false` menandai hasil parsial.
