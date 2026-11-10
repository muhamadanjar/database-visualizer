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

| Kategori            | Perintah                                                                                                                                                | Hasil lokal                                                                                                        |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Rust unit/integrasi | `cargo test -p db-core -p db-types -p desktop-runtime -p query-runtime -p studio-api --features studio-api/test-provider --locked -- --include-ignored` | 62 tests lulus; termasuk PostgreSQL/MySQL nyata, backup/migrasi, OS-store paths, spool disk-full, dan API security |
| Rust lint           | `cargo clippy --workspace --all-targets --locked -- -D warnings`                                                                                        | Lulus; empat warning upstream SQLx PostgreSQL tetap dicatat                                                        |
| Frontend            | `pnpm test`                                                                                                                                             | 20 tests lulus                                                                                                     |
| Static              | `cargo fmt --all --check`, `pnpm typecheck`, `pnpm typecheck:web`, `pnpm format:check`                                                                  | Lulus                                                                                                              |
| Native Linux        | `pnpm test:native`                                                                                                                                      | 2 engine E2E + 2 restart tests lulus; PostgreSQL/MySQL; WebKit/Tauri                                               |
| Paket Linux         | `pnpm --filter @database-studio/desktop tauri build --debug --bundles deb`                                                                              | .deb amd64 79,03 MiB; default production features, unsigned/debug                                                  |
| Katalog core        | `cargo run -p db-core --locked --example catalog_benchmark`                                                                                             | Run 2026-11-09: 10.000 object PostgreSQL; metadata 326 ms; DTO 1.330.082 bytes; setup 6.082 ms                     |

Native E2E mencakup create/test/connect, browse/Columns/DDL/Data, clipboard OS, script berurutan, nilai lossless, transaksi, cancellation, disconnect/delete, dan grid 100.000 rows dengan scroll/filter/reset. Pengisian Monaco memakai helper test-only yang memanggil editor nyata; database dan transport tidak dimock. Fixture disposable menggunakan konfigurasi port repository; credential fixture bukan credential pengguna.

Pengukuran native debug dari Run sampai tampilan/scroll/validasi 100.000 rows pada run terbaru: PostgreSQL 3.839 ms dan MySQL 3.550 ms; masing-masing 17 DOM rows pada viewport 900×1028, DPR 1. Angka mencakup overhead WebdriverIO dan bukan durasi query, benchmark release, atau bukti tidak pernah freeze. Benchmark 10.000 object masih mengukur metadata core saja, bukan navigasi UI.

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
- Probe OS credential store `credential_probe` berhasil menyimpan, membaca ulang setelah membuka `Persistence` baru, menghapus, dan memverifikasi secret hilang pada Linux (`OS_STORE_PERSISTENCE_REOPEN_AND_DELETE_OK`). Native E2E benar-benar restart app dan membaca profil/password dari OS store: PostgreSQL dan MySQL pada run penuh sebelumnya; PostgreSQL diulang pada 2026-11-09. Locked/missing store native, history berbasis keyring, dan recovery lintas store masih belum terbukti menyeluruh.
- `pnpm --filter @database-studio/desktop tauri build --debug --bundles deb` berhasil menghasilkan paket Linux `.deb` 79,01 MiB. Ini build debug, belum dipasang atau dijalankan sebagai paket production; peringatan 4 item vendored SQLx PostgreSQL dan ukuran chunk editor Vite tetap tercatat.

Status tetap **Berjalan**. Stress slow ACK native kini lulus di Linux, tetapi consumer-close dan disk-full native belum diuji. Pengukuran idle whole-process WebView kini tersedia dan berada di atas target 200 MiB; cold startup dan navigasi UI 10.000 objek belum terukur. Windows/macOS, paket/runner tiap OS, pipeline matrix, dan locked/missing store masih terbuka. Hasil native berasal dari build automation/debug, bukan bukti paket production terpasang.

## Perbaikan blocker lanjutan — 2026-11-09

- Root cause ACK: watchdog hanya menunggu credit saat signature snapshot berubah. Query lama yang stabil menahan credit tanpa deadline; walau jalur cancel manual berhasil, batas 30 detik tidak berjalan. Watch kini menunggu ACK langsung setelah tiap event nonterminal dan membatalkan query saat credit tak kembali; terminal dapat diambil lewat watch baru. Review juga menemukan race rewatch saat cleanup watcher lama, kini map entry memakai identitas watcher dan timeout menghapus watcher aktif sebelum cancel; cleanup lama tidak dapat menghapus pengganti.
- Native stress E2E PostgreSQL nyata: `ACK_STRESS {"unacked_events":1,"terminal_status":"Cancelled","rewatch_events":1}`; skenario lulus setelah no-ACK cancellation dan pembacaan terminal melalui watch baru. Query berjalan dibatalkan setelah timeout 30 detik. Full Linux E2E kedua engine meluluskan UPDATE/Refresh, cancel, transaksi, disconnect, dan restart OS credential; grid 100.000 rows diukur 3.839 ms PostgreSQL dan 3.550 ms MySQL, masing-masing 17 DOM rows pada viewport 900×1028.
- Pengukuran idle build debug Linux: 426.776 KiB seluruh pohon proses pada gate idle; restart: 429.164 KiB. Ini melampaui target PRD <200 MiB. Startup tetap tidak tersedia karena launcher WebDriver tidak menyediakan timestamp launch; angka bukan cold-start measurement/release build.
- Disk penuh dibuktikan oleh uji spool `/dev/full`: write gagal tanpa merusak baris terenkripsi sebelumnya dan reservation kuota dipulihkan. Belum merupakan pengujian native app dengan disk penuh.
- Native smoke penggunaan OS credential store melakukan restart PostgreSQL dan memulihkan profil serta password. Probe `credential_probe` membuka ulang persistence, membaca secret OS, menghapus credential dan memastikan pembacaan berikutnya hilang. Tidak ada bukti locked store di Windows/macOS atau pemulihan semua kombinasi SQLite/OS store.
- Pipeline matrix belum dapat dijalankan; workflow meminta self-hosted desktop runner interaktif `database-studio-native` dengan keyring unlocked. GitHub CLI API gagal autentikasi (`401`), sehingga ketersediaan runner dan hasil CI tidak diketahui.
- Paket debug `.deb` amd64 berhasil dibangun dari default non-automation configuration; `dpkg-deb --info` mengenali package `database-studio`, version `0.1.0`, architecture `amd64`. Bundle frontend production tidak mengandung `__DATABASE_STUDIO_WATCH_NO_ACK__`, `__wdio_invoke`, atau `@wdio/tauri-plugin`. Paket tidak dipasang atau dijalankan, dan bukan signing/release proof.

## Consumer-close native E2E — bukti run sebelumnya, 2026-11-09

- `pnpm test:native` lulus di Linux/Arch dengan WebKitGTK 605.1.15 dan PostgreSQL/MySQL nyata. Hasil native mencakup dua skenario engine dan dua restart tests; harness membangun app debug dengan automation feature, bukan paket `.deb`.
- E2E PostgreSQL menjalankan query lambat, memasang watcher tanpa ACK, lalu me-reload WebView untuk menutup consumer. Watchdog membatalkan query; query teramati berstatus `Cancelled`; watcher baru mengambil event terminal. Bukti log: `CONSUMER_CLOSE {"terminal_status":"Cancelled","rewatch_events":1}`.
- Ini membuktikan teardown/consumer kehilangan ACK dan pemulihan terminal pada konfigurasi Linux ini; tidak membuktikan perilaku macOS/Windows atau jalur send-error spesifik di semua platform. macOS sengaja tidak diuji dari lingkungan Linux. Idle whole-process RSS pada run: 424.644 KiB; startup tidak terukur.

- Run sebelumnya setelah perubahan productivity, 2026-11-09: fase native awal lulus pada PostgreSQL dan MySQL; fase restart berhenti sebelum skenario dimulai karena gate idle RSS tidak menghasilkan sample. Command `pnpm test:native` karena itu exit 1 dan tidak membuktikan restart pada run tersebut. Bukti consumer-close di atas berasal dari run sebelumnya. macOS dan `.deb` tidak diuji/dibangun ulang dari lingkungan Arch ini.

## Validasi fitur native — 2026-11-09

- `pnpm test:native` terbaru lulus pada Linux/Arch: skenario PostgreSQL dan MySQL serta dua restart tests. E2E membuktikan inline update/delete/insert dan refresh halaman setelah Apply, add-column DDL dan Inspector refresh pada kedua engine; fixture memvalidasi table metadata sebelum navigasi.
- Run juga meluluskan ACK no-ACK cancellation (`ACK_STRESS`) dan consumer-close recovery (`CONSUMER_CLOSE`). Ini build debug dengan automation feature, bukan paket release.
- Whole-process idle RSS: 440.684 KiB pada run awal dan 432.664 KiB setelah restart; cold startup tidak terukur. macOS dan `.deb` sengaja tidak diuji/dibangun dari Arch Linux.
