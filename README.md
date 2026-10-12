# Database Studio

Database Studio adalah client PostgreSQL/MySQL desktop dan web yang direncanakan dalam [PRD](PRD.md). Repository menyediakan shared Rust core dan Desktop MVP Tauri/React: koneksi, explorer/inspector, SQL editor, streaming results, cancellation, transaksi, dan penyimpanan lokal terenkripsi. Web/Axum Phase 3 tersedia; acceptance runtime/deployment masih terbuka. Dokumentasi fase: [docs](docs/README.md).

## Struktur & dependency

- `crates/db-types`: typed config, capabilities, metadata, values, events, errors.
- `crates/db-core`: adapter contracts, session registry, resource limits, SQL validation, bounded streaming, cancellation/cleanup.
- `crates/db-postgres`, `crates/db-mysql`: driver, introspection, decoding, control cancellation.
- `crates/desktop-runtime`: layanan desktop testable tanpa WebView: SQLite, OS store, script orchestration, spool terenkripsi.
- `apps/desktop`: bootstrap React/Vite dan host Tauri 2.
- `packages/ui`, `packages/shared-types`: UI dan kontrak transport reusable.
- `vendor/sqlx-{postgres,mysql}`: patch 0.8.6 yang disetujui, accessor status protocol dan pin tujuan TCP yang mempertahankan hostname TLS.
- `infra/docker`: initialization untuk database test PostgreSQL non-superuser.
- `compose.yaml`: PostgreSQL customer, MySQL customer, dan PostgreSQL aplikasi terpisah.

Adapters bergantung pada kontrak core; core tidak mengimpor adapters, SQLx, UI, Tauri, Axum, atau network transport. Example/tests menghubungkan implementasi konkret.

## Setup & commands

Butuh Docker Compose dan Rust 1.99.0 (lihat `rust-toolchain.toml`). Dependency versions/graph dikunci dalam manifests dan `Cargo.lock`; database image digests juga dikunci. Versi yang diuji: PostgreSQL 16.13, MySQL 8.4.8, SQLx 0.8.6, sqlparser 0.63.0.

```sh
docker compose up -d --wait
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --locked -- --include-ignored
cargo run -p db-core --locked --example foundation -- postgres
cargo run -p db-core --locked --example foundation -- mysql
```

`cargo test` biasa menjalankan policy/unit tests tanpa database. Integration tests ditandai `ignored` karena harus diarahkan ke fixture disposable; `--include-ignored` menjalankan seluruh suite, termasuk spike driver/parser. Example menampilkan metadata count dan SELECT streamed melalui core. Desktop tersedia; browser preview tidak memiliki database transport.

Default fixture customer: PostgreSQL `127.0.0.1:55432`, MySQL `127.0.0.1:53306`, user `studio`, database `studio_test`. PostgreSQL aplikasi memakai port `55433` dan user/database `studio_app`. Credential dalam `.env.example` adalah fixture lokal disposable, bukan production secret. Untuk custom port/password, export `STUDIO_PG_PORT`, `STUDIO_MYSQL_PORT`, `STUDIO_APP_PORT`, `STUDIO_TEST_PASSWORD` ke shell; Compose dan example membacanya. Integration tests membaca variable yang sama. `.env` Compose tidak otomatis diekspor ke proses Rust.

PostgreSQL test user adalah non-superuser; MySQL hanya diberi akses ke database fixture. Tests dapat membuat/menghapus tabel, view, dan sequence dengan nama unik. Jangan arahkan ke database pengguna/produksi. `docker compose down` menghentikan environment proyek ini; data disposable hilang ketika container direkreasi karena belum ada persistent volume.

## Desktop: development & packaging

Butuh Node 24, pnpm 10.33.0, Rust 1.99.0, dan prerequisite native Tauri. Linux memerlukan GTK 3, WebKitGTK 4.1, Ayatana AppIndicator, librsvg, serta patchelf untuk packaging. macOS memerlukan Xcode command-line tools; Windows memerlukan MSVC build tools dan WebView2.

```sh
pnpm install --frozen-lockfile
pnpm desktop
pnpm typecheck
pnpm test
pnpm format:check
pnpm test:native
pnpm --filter @database-studio/desktop tauri build --debug --bundles deb
```

`pnpm dev` hanya preview browser; Rust host diperlukan untuk akses database. Native E2E membutuhkan desktop/display dan kedua fixture Compose. Di Linux CI gunakan `xvfb-run -a pnpm test:native`. Runner membuat app-data sementara dan automation build terpisah; plugin/permissions WDIO serta helper editor E2E tidak aktif pada production build. Screenshot berada di `test-results/native/`.

TLS default VerifyFull; fixture lokal tanpa sertifikat perlu memilih Disabled secara eksplisit. Password dapat disimpan di OS credential store atau hanya memori selama aplikasi berjalan. Store locked/unavailable menghasilkan error aman; tidak ada fallback plaintext. History/restore editor default OFF per koneksi; saved query eksplisit. SQL di SQLite memakai XChaCha20-Poly1305 dengan master key OS. Missing key tidak diganti ketika ciphertext sudah ada.

Layout dan metadata recent files disimpan tanpa isi SQL. Open/save file memakai dialog native; save file menulis teks UTF-8 di lokasi yang dipilih pengguna. File pengguna ini terpisah dari SQL terenkripsi milik aplikasi. Restore membuka teks tanpa koneksi/replay; maksimum 16 editor/koneksi dan 64/workspace. History 30 hari/1.000 eksekusi; list 100 header/halaman dan SQL ditarik per dokumen.

Hasil berada dalam encrypted ephemeral spool, 1 GiB/hasil dan 4 GiB/aplikasi; close/startup cleanup menghapusnya. Pages maksimal 256 rows/2 MiB, cache hasil 32 MiB/tab/128 MiB/aplikasi. Sorting lokal dibatasi 32 MiB; hasil besar meminta SQL ORDER BY eksplisit. Batas ini menghitung payload, bukan total RSS. Metadata inspector PostgreSQL table DDL adalah rekonstruksi untuk inspeksi, bukan backup/migration script.

SQLite schema v2 memakai migrasi transaksi, backup sebelum migrasi, dan penolakan schema lebih baru. Recovery: tutup aplikasi, amankan salinan directory data lokal, pulihkan backup dengan versi aplikasi yang sesuai; OS credential/key harus dipulihkan terpisah. Jangan menurunkan `user_version` secara manual atau menghapus ciphertext untuk melewati error.

## Runtime contract

Satu editor session memegang koneksi tetap dan maksimal satu query aktif. Session sibuk ditolak; reconnect membuat session baru tanpa replay atau pemulihan transaksi. Tidak ada idle eviction. Gunakan `disconnect`/`shutdown`; host terakhir yang di-drop memberi sinyal cleanup ke query aktif. Host production harus melakukan shutdown terawait sebelum menghentikan runtime Tokio.

Default: editor 4/profil dan 16/proses; metadata serta kontrol cancellation masing-masing 1/profil dan 4/proses. Kuota menghitung koneksi yang sedang dibuka. Metadata memakai koneksi terpisah dan tidak melihat transaksi/temp table editor. Profil diidentifikasi oleh `ConnectionConfig.id`; host harus menjaga ID dan ownership profil.

Batch maksimal 256 rows atau 1 MiB payload; antrean 4 MiB/query plus batas jumlah pesan. Producer menunggu ketika penuh; tertahan 30 detik memicu cancellation. Row terlalu besar gagal tanpa truncation. Byte accounting menghitung payload nilai/column metadata, bukan allocator overhead; pending batch dan current driver row berada di luar antrean. SQLx dapat mengalokasikan message/row besar sebelum ukuran diperiksa: ini bukan batas keras total RSS.

Connect timeout 10 detik pada contoh/config; query timeout opsional. `cancel(query_id)` mengajukan permintaan, bukan bukti query berhenti. Event terminal diterbitkan sekali; completion yang menang race tetap Completed. Summary `results_complete=false` menandakan rows dibuang saat cancellation walaupun driver melaporkan sukses; consumer harus menampilkan hasil parsial. Cancellation/cleanup mempunyai satu deadline 5 detik. Ketidakpastian menjadi `QUERY_OUTCOME_UNKNOWN` dan session invalid. Failed/Cancelled tidak menjamin rollback; rollback tidak dilakukan diam-diam. Setelah synchronize, status transaksi berasal dari protocol PostgreSQL I/T/E dan MySQL IN_TRANS. Busy/invalid session tetap Unknown; wrapper depth SQLx tidak dipakai.

## SQL, values & security boundaries

SQL asli dikirim tanpa rewrite setelah validasi parser dialect. Foundation hanya menerima satu statement dan menolak CALL, PREPARE/EXECUTE, COPY serta MySQL executable comments; blok DO PostgreSQL belum didukung parser. Syntax database valid yang tidak dipahami parser mendapat `UNSUPPORTED_STATEMENT`. Desktop memecah script dengan parser dialect/source spans, memvalidasi seluruh script sebelum mulai, lalu menjalankan statement berurutan tanpa rewrite, retry, atau transaksi otomatis. Batas 1 MiB/1.000 statements.

Nilai mempertahankan NULL, integer, float (string round-trip), exact decimal, text/binary, temporal, JSON, atau opaque engine type/format/payload. Unsupported/invalid decode tidak dipotong atau diubah menjadi nilai tebakan. Metadata tetap menyimpan nama tipe engine. MySQL BOOLEAN adalah alias TINYINT(1); nilai selain 0/1 tetap integer. Temporal offsets mengikuti tipe asli; jangan menganggap timestamp tanpa offset sebagai UTC.

Connection config tidak serializable dan Debug redacted; driver statement logging dimatikan. Error publik hanya pesan aman dan database code, tanpa SQL/driver message/credential. Host wajib mengisi `ProtectedTarget` dengan seluruh endpoint/alias control plane yang harus ditolak. Guard ini membandingkan endpoint konfigurasi, bukan DNS-aware tenant authorization; host web nanti harus memvalidasi ownership dan kebijakan target di backend. Core memakai koneksi customer terpisah, bukan application pool.

## Evidence & limits

Lihat [Phase 1 validation](docs/plans/phase-1-foundation.md). Driver spike membuktikan session/temp table, cancellation/recovery dengan akun biasa, dan batas parser. Integration tests meliputi metadata, typed values, streaming, backpressure, oversized row, blocked lock, reconnect, consumer drop. Failure-path policy tests memakai adapter terkontrol dan tidak menggantikan bukti database.

Lihat [evidence Desktop](docs/plans/phase-2-desktop-mvp-validation.md). Linux native E2E memeriksa database nyata; ini belum membuktikan Windows/macOS, OS credential store yang tersedia/locked, signing, pipeline, TLS produksi, seluruh tipe engine, atau hard RSS ceiling. Gate Phase 2 masih dicatat secara terpisah.

## Web MVP

`apps/web` memakai shared UI melalui HTTP/WS; `services/api` menyediakan Axum, scoped PostgreSQL persistence, auth dan query runtime. `crates/query-runtime` menjadi pemilik bersama script/spool/profile/DTO. Kontrak: [OpenAPI](services/api/openapi.json).

Gunakan [runbook](docs/plans/phase-3-web-mvp-operations.md) untuk keys/policy, migration terpisah, API, lalu `pnpm web`. Validasi lokal: `pnpm typecheck:web`, `pnpm build:web`, `pnpm test`, dan `cargo test -p studio-api -p query-runtime --locked`. Feature fixture `test-provider` tidak aktif pada build production. Deployment operator: `compose.web.yaml`.

[Laporan Phase 3](docs/plans/phase-3-web-mvp-validation.md) mencatat bukti dan gate API/browser/OAuth/TLS/Docker/performa yang belum lulus. Perintah implementasi bukan izin deployment production.

Phase 4 productivity/editing tersedia; lihat [fitur](docs/features/productivity-tools.md), [kontrak](docs/design-docs/phase-4-productivity-contracts.md) dan [validasi](docs/plans/phase-4-productivity-validation.md). Jalankan `node scripts/productivity-benchmark.mjs` untuk pengukuran completion lokal. API schema v2 memerlukan migration terpisah dan backup operator; Unknown mutation tidak boleh direplay.
