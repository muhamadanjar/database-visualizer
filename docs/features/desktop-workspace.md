# Desktop Workspace & Local Persistence

**Status implementasi:** Desktop Linux tersedia dan native E2E lulus; gate tiga OS/persistence native masih terbuka
**Fase:** Phase 2
**Sumber:** [PRD](../../PRD.md) §§5, 21, 28–33, 39–40, 44, 49, 53.

## Tujuan

Aplikasi Tauri 2 yang mengakses database dari mesin pengguna dan mempertahankan workspace lokal.

## Perilaku & cakupan

- React/TypeScript berkomunikasi melalui Tauri IPC ke Rust host yang memakai shared core.
- SQLite menyimpan connection metadata, settings, query history, workspace state, recent files, dan saved queries.
- OS credential storage menyimpan secret; frontend tidak mendapat akses shell/filesystem arbitrer.
- Tauri permissions dibatasi pada kebutuhan aplikasi; UI berisi toolbar, explorer, editor, results/messages/explain, status bar.
- History default nonaktif dan dapat diaktifkan per connection; saved query dapat dibuat, diedit, dihapus, dan diubah namanya.

## Acceptance criteria

- Alur desktop PRD §53 lulus pada PostgreSQL/MySQL, termasuk transaksi dan history.
- State tab/workspace dan secret reference tersimpan sesuai desain; plaintext password tidak masuk SQLite.
- Startup <2 detik dan idle memory <200 MB menjadi target benchmark yang dilaporkan dengan kondisi pengukuran.

## Batasan & keputusan lanjutan

Desain Phase 2 menetapkan history/restore teks default OFF dan opt-in per koneksi; SQL persisten terenkripsi, saved queries eksplisit, dan layout restore tanpa auto-connect/replay. History dibatasi 30 hari/1.000 eksekusi per koneksi. OS store gagal menawarkan memory-only tanpa plaintext fallback. SQLite memakai migrasi bernomor, backup, dan schema-version guard.

Windows/macOS/Linux menjadi gate build, credential-store checks, dan native E2E WebdriverIO Tauri. Schema 1, OS backend eksplisit, dan dependency lock tersedia; recovery dan OS store nyata memerlukan validasi platform. SSH, signing, serta auto-update ditunda; paket unsigned hanya untuk validasi lokal.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md) menetapkan Q1–Q18 dan gate; [Foundation](../design-docs/2026-10-09-foundation.md) menetapkan baseline core.


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.
