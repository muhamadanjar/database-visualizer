# Phase 2 — Bukti validasi lokal

**Tanggal:** 2026-10-10 (benchmark katalog: 2026-10-09).
**Status:** implementasi lokal Linux tersedia; acceptance Phase 2 masih Berjalan.
**Terkait:** [plan](phase-2-desktop-mvp.md), [desain Approved](../design-docs/2026-10-09-desktop-mvp.md), [review driver](../design-docs/phase-2-driver-status-review.md).

## Artefak implementasi

- `crates/desktop-runtime/`: registry session, script worker, encrypted spool, SQLite schema 1/backup, OS credentials, dan shutdown; independen dari WebView.
- `apps/desktop/src-tauri/`: Tauri commands, Channel/ACK, native dialog/clipboard, capabilities production, serta feature automation terpisah.
- `packages/shared-types/` dan `packages/ui/`: IPC v1/lossless values, Monaco, tabs, explorer/inspector, table viewer, grid virtualized, saved queries/history.
- `vendor/sqlx-{postgres,mysql}/`: versi 0.8.6, license/provenance; hanya dua accessor status read-only yang disetujui pengguna.
- `scripts/desktop-e2e.mjs`, `tests/native/`, dan dua workflow desktop: runner dan konfigurasi validasi; workflow bukan bukti pipeline sudah berjalan.

## Hasil dan perintah

| Kategori | Perintah | Hasil lokal |
|---|---|---|
| Rust unit/integrasi | `cargo test -p db-core -p db-types -p desktop-runtime --locked -- --include-ignored` | 40 tests lulus dengan PostgreSQL/MySQL nyata |
| Rust lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lulus; empat warning upstream SQLx PostgreSQL tetap dicatat |
| Frontend | `pnpm test` | 5 tests, 3 files lulus; controlled transport, bukan native |
| Static | `cargo fmt --check`, `pnpm typecheck`, `pnpm format:check` | Lulus |
| Native Linux | `pnpm test:native` | 2 tests lulus di WebKit/Tauri, kedua engine, 25,3 detik waktu test |
| Paket Linux | `pnpm --filter @database-studio/desktop tauri build --debug --bundles deb` | .deb amd64 90,98 MiB; default production features, unsigned/debug |
| Katalog core | `cargo run -p db-core --locked --example catalog_benchmark` | 10.000 object PostgreSQL; metadata 291 ms; DTO 1.330.067 bytes; setup 9.647 ms |

Native E2E mencakup create/test/connect, browse/Columns/DDL/Data, clipboard OS, script berurutan, nilai lossless, transaksi, cancellation, disconnect/delete, dan grid 100.000 rows dengan scroll/filter/reset. Pengisian Monaco memakai helper test-only yang memanggil editor nyata; database dan transport tidak dimock. Fixture disposable menggunakan konfigurasi port repository; credential fixture bukan credential pengguna.

Pengukuran debug native dari Run sampai tampilan/scroll/validasi 100.000 rows: PostgreSQL 4.580 ms, MySQL 7.470 ms; masing-masing hanya 12 DOM rows dirender. Viewport 900×842 CSS pixels, DPR 2. Angka mencakup overhead WebdriverIO dan bukan durasi query, benchmark release, atau bukti tidak pernah freeze. Benchmark katalog hanya core, belum navigasi UI 10.000 object.

Log sesi berada di `/tmp/database-studio-final-tests.log`, `/tmp/database-studio-final-clippy.log`, `/tmp/database-studio-ui-tests.log`, `/tmp/database-studio-native-full.log`, dan `/tmp/database-studio-catalog-benchmark.log`. Screenshot terbaru: `test-results/native/{postgres,mysql}-workspace.png`. Log/artifact ini lokal, tidak di-commit; perintah di atas menghasilkan ulang bukti.

## Security, persistence, dan batas resource

Controlled-store tests membuktikan ciphertext, missing key tanpa regenerasi, rotasi secret gagal tanpa kehilangan metadata, cleanup tertunda, schema guard/backup, paginasi saved-query headers tanpa SQL, dan restore tanpa auto-connect. Probe OS store Linux nyata menghasilkan `SECRET_STORE_UNAVAILABLE`; persistence history/saved queries dengan store native yang unlocked belum terbukti pada mesin ini.

Script dibatasi 1 MiB/1.000 statements; page 256 rows/2 MiB; notifikasi satu credit/ACK, timeout 30 detik. Cache aplikasi 32 MiB/tab dan 128 MiB/global; spool 1 GiB/result dan 4 GiB/global. Sorting lokal memiliki budget 32 MiB dan meminta ORDER BY ketika terlalu besar. Unit tests quota/cleanup dan integrasi backpressure core bukan pengganti stress ACK native.

## Gate yang masih terbuka

- Windows/macOS: build/package, native E2E, credential store nyata, serta close/restart/recovery.
- Linux: credential store tersedia/locked/missing nyata dan opt-in history/restore end-to-end.
- Recovery lintas SQLite/OS store setelah crash/migration failure; close aplikasi dengan transaksi aktif secara native.
- Slow ACK/consumer close/disk-full stress native serta pengukuran RSS seluruh proses WebView.
- Cold startup <2 detik, idle memory <200 MB, dan navigasi UI 10.000+ object.
- Eksekusi pipeline matrix; workflow manual memerlukan self-hosted runner interaktif berlabel `database-studio-native` dengan fixture dan keystore unlocked.

Paket lokal unsigned tidak membuktikan signing, auto-update, distribusi publik, atau seluruh target performa. Implementasi web/Axum/tenant tetap Phase 3.

Paket: `target/debug/bundle/deb/Database Studio_0.1.0_amd64.deb`. `dpkg-deb --info` mengenali package/version/dependency; dependency tree production tidak memuat WDIO dan frontend dist tidak memuat helper editor/automation. Paket belum diinstal sistem.

Percobaan launch binary production melalui shell sandbox pada 2026-10-10 berhenti sebelum window karena GTK tidak dapat mengakses display (juga saat DISPLAY=:1). Ini belum membuktikan launch paket production; native E2E di atas adalah build automation terpisah. Pengukuran startup/idle dari percobaan ini tidak tersedia. Log: `/tmp/database-studio-production-smoke.log`.
