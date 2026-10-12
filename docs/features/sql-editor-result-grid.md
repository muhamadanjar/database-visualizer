# SQL Editor & Result Grid

**Status implementasi:** Monaco, script, transaksi, dan grid 100.000 rows tervalidasi native Linux; acceptance tiga OS masih terbuka
**Fase:** Phase 2: desktop; Phase 3: reuse web; Phase 4: peningkatan
**Sumber:** [PRD](../../PRD.md) §§16–18, 31–33, 36, 40.

## Tujuan

Workspace SQL Monaco dengan beberapa tab dan result grid yang tetap responsif saat menerima banyak rows.

## Perilaku & cakupan

- Jalankan current statement, selection, atau full editor; dukung dialect PostgreSQL/MySQL.
- MVP: highlighting, formatting dasar, keyword/table/column autocomplete, shortcuts, errors, durasi, affected rows, Run/Cancel.
- Tab mempertahankan text/cursor/selection dan undo history sebisa mungkin; panel dapat diubah ukurannya.
- Grid memakai row virtualization dan column virtualization bila diperlukan; tampilkan NULL, header, selection, copy cell/rows/CSV.
- TanStack Query untuk server state dan Zustand untuk UI state; UI memakai transport bersama tanpa database driver.
- Basic EXPLAIN tampil sebagai table/text pada MVP. Buffer hasil frontend harus terbatas.
- Desain Phase 2: script preflight lalu berurutan pada session tab; stop-first-error/cancel tanpa rewrite, retry, atau transaksi otomatis. EXPLAIN tidak menjalankan ANALYZE otomatis.
- Host memiliki spool terenkripsi, notifikasi/ACK terbatas, dan page pull. Cache awal 32 MiB/tab, 128 MiB/aplikasi; spool 1 GiB/hasil, 4 GiB/aplikasi; hasil parsial ditandai.
- Sorting/filtering grid SQL berlaku pada rows diterima; operasi seluruh hasil tidak memuat semuanya ke frontend. Session dibuka lazy per tab dan reconnect tidak replay.

## Acceptance criteria

- Alur execute/stream/cancel berfungsi di desktop dan web untuk kedua engine.
- Frontend tests mencakup execution state, tabs, connection forms, tree, dan grid.
- Target 100.000+ returned rows diuji dengan benchmark serta batas memory yang terdokumentasi.

## Batasan & keputusan lanjutan

Advanced language engine, AI assistant, debugger, dan collaboration bukan MVP.

Status protocol authoritative, IPC limits, dan encrypted spool telah diterapkan dan diuji lokal; benchmark lengkap dan stress ACK native masih terbuka. Vitest/React Testing Library menguji frontend; native WebdriverIO Tauri menguji integrasi desktop. Gate pada desain wajib diselesaikan sebelum fitur terkait selesai.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md) dan [Foundation](../design-docs/2026-10-09-foundation.md).


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.

## Phase 4 — Implementasi berjalan

[Productivity](../design-docs/2026-10-10-productivity.md) menetapkan completion konteks umum/cache terbatas, navigasi EXPLAIN/DDL, shortcuts sesuai fokus dan staged CRUD grid hanya pada base table transactional ber-PK. Editing memerlukan typed values, scoped snapshots, conflict checking serta session/transaksi khusus; hasil query arbitrer tetap read-only. [Checklist](../plans/phase-4-productivity.md) memisahkan review/desain dari implementasi dan bukti runtime.

2026-10-11: completion alias/statement/dialect, cache metadata terbatas, DDL text search/copy dan editor tabel tersedia. Table editing berhalaman 100 row memakai PK/composite, original snapshot, typed values, preview/confirm dan dedicated transaction; draft hanya memori. Regresi kedua engine lulus; [validasi Phase 4](../plans/phase-4-productivity-validation.md) memisahkan DB/unit dari browser/native dan acceptance terbuka.
