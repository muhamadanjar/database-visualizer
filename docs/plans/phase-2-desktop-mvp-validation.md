# Phase 2 — Bukti validasi lokal

**Tanggal:** 2026-10-10 (benchmark katalog: 2026-10-09); regresi tambahan 2026-11-08 di bagian akhir.
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
- Linux: store OS unlocked berhasil untuk round-trip/delete pada 2026-11-08; locked/missing nyata, restart native, serta opt-in history/restore end-to-end masih terbuka.
- Recovery lintas SQLite/OS store setelah crash/migration failure; close aplikasi dengan transaksi aktif secara native.
- Slow ACK/consumer close/disk-full stress native serta pengukuran RSS seluruh proses WebView.
- Cold startup <2 detik, idle memory <200 MB, dan navigasi UI 10.000+ object.
- Eksekusi pipeline matrix; workflow manual memerlukan self-hosted runner interaktif berlabel `database-studio-native` dengan fixture dan keystore unlocked.

Paket lokal unsigned tidak membuktikan signing, auto-update, distribusi publik, atau seluruh target performa. Implementasi web/Axum/tenant tetap Phase 3.

Paket: `target/debug/bundle/deb/Database Studio_0.1.0_amd64.deb`. `dpkg-deb --info` mengenali package/version/dependency; dependency tree production tidak memuat WDIO dan frontend dist tidak memuat helper editor/automation. Paket belum diinstal sistem.

Percobaan launch binary production melalui shell sandbox pada 2026-10-10 berhenti sebelum window karena GTK tidak dapat mengakses display (juga saat DISPLAY=:1). Ini belum membuktikan launch paket production; native E2E di atas adalah build automation terpisah. Pengukuran startup/idle dari percobaan ini tidak tersedia. Log: `/tmp/database-studio-production-smoke.log`.

## Regresi desktop — 2026-11-08

- Koneksi baru kini mencentang simpan password di OS store; pengguna dapat memilih memory-only. Koneksi lama memory-only harus diedit dan password dimasukkan kembali agar dapat dipakai setelah restart. Pindah dari OS store ke memory-only menolak password kosong, menghapus reference/secret OS setelah password baru dimasukkan, dan tidak diam-diam mempertahankan secret. Simpan dengan store terkunci/tidak tersedia gagal tanpa plaintext fallback. Regresi `desktop-runtime/tests/security.rs` membuktikan penyimpanan setelah reopen persistence dan penghapusan credential saat pindah mode pada controlled-store; belum membuktikan restart aplikasi dengan OS store nyata.
- UI menampilkan affected/returned rows untuk hasil UPDATE serta error database dari terminal statement gagal, termasuk kegagalan setelah statement sebelumnya berhasil. Refresh Inspector Data meminta ulang metadata dan halaman tabel. Alur native mengeksekusi UPDATE pada SQL tab lalu membaca nilai baru setelah Refresh pada PostgreSQL dan MySQL.
- `cargo test -p db-core -p db-types -p desktop-runtime -p query-runtime --locked -- --include-ignored`: 51 tes lulus dengan fixture disposable kedua engine; `pnpm test`: 20 tes setelah tambahan failure-path UI; `pnpm test:native`: dua skenario lulus di WebKit/Tauri Linux dengan fixture nyata, termasuk UPDATE dan refresh. `pnpm typecheck`, `pnpm format:check`, `cargo fmt --all --check` dan workspace Clippy diperiksa terpisah. Empat warning vendored SQLx PostgreSQL bukan kegagalan workspace.
- Probe OS credential store dari contoh `credential_probe` melaporkan `OS_STORE_ROUNDTRIP_AND_DELETE_OK` pada sesi Linux ini. Native E2E berhasil membuat koneksi dengan opsi remember default baru, tetapi belum menguji restart nyata, store locked/missing, history/restore berbasis keyring, atau recovery lintas OS store/SQLite.
- `pnpm --filter @database-studio/desktop tauri build --debug --bundles deb` berhasil menghasilkan paket Linux `.deb` 79,01 MiB. Ini build debug, belum dipasang atau dijalankan sebagai paket production; peringatan 4 item vendored SQLx PostgreSQL dan ukuran chunk editor Vite tetap tercatat.

Status tetap **Berjalan**: belum ada bukti Windows/macOS, paket/runner tiap OS, pipeline matrix, stress ACK/disk-full, cold startup <2 detik, idle memory <200 MB, RSS WebView, maupun navigasi UI 10.000 objek. Hasil native berasal dari build automation/debug; bukan bukti paket production terpasang.
