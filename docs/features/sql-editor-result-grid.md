# SQL Editor & Result Grid

**Status implementasi:** Belum dimulai  
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

## Acceptance criteria

- Alur execute/stream/cancel berfungsi di desktop dan web untuk kedua engine.
- Frontend tests mencakup execution state, tabs, connection forms, tree, dan grid.
- Target 100.000+ returned rows diuji dengan benchmark serta batas memory yang terdokumentasi.

## Batasan & keputusan lanjutan

Advanced language engine, AI assistant, debugger, dan collaboration bukan MVP.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.
