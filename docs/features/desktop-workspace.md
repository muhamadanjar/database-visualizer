# Desktop Workspace & Local Persistence

**Status implementasi:** Belum dimulai  
**Fase:** Phase 2  
**Sumber:** [PRD](../../PRD.md) §§5, 21, 28–33, 39–40, 44, 49, 53.

## Tujuan

Aplikasi Tauri 2 yang mengakses database dari mesin pengguna dan mempertahankan workspace lokal.

## Perilaku & cakupan

- React/TypeScript berkomunikasi melalui Tauri IPC ke Rust host yang memakai shared core.
- SQLite menyimpan connection metadata, settings, query history, workspace state, recent files, dan saved queries.
- OS credential storage menyimpan secret; frontend tidak mendapat akses shell/filesystem arbitrer.
- Tauri permissions dibatasi pada kebutuhan aplikasi; UI berisi toolbar, explorer, editor, results/messages/explain, status bar.
- History dapat dinonaktifkan per connection; saved query dapat dibuat, diedit, dihapus, dan diubah namanya.

## Acceptance criteria

- Alur desktop PRD §53 lulus pada PostgreSQL/MySQL, termasuk transaksi dan history.
- State tab/workspace dan secret reference tersimpan sesuai desain; plaintext password tidak masuk SQLite.
- Startup <2 detik dan idle memory <200 MB menjadi target benchmark yang dilaporkan dengan kondisi pengukuran.

## Batasan & keputusan lanjutan

Matriks pengujian Windows/macOS/Linux serta schema/migration SQLite belum diputuskan. Code signing/auto-update dapat ditunda.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.
