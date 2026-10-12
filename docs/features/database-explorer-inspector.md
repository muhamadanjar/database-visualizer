# Database Explorer, Inspector & Table Viewer

**Status implementasi:** Metadata lengkap kedua engine dan inspector desktop tervalidasi lokal Linux; benchmark katalog UI belum lulus
**Fase:** Phase 1: metadata; Phase 2–3: UI desktop/web
**Sumber:** [PRD](../../PRD.md) §§10, 13–15, 40, 56.

## Tujuan

Menjelajah object database dan memeriksa struktur/data tanpa memuat seluruh katalog atau tabel.

## Perilaku & cakupan

- Explorer berbentuk tree dengan lazy loading per node; capabilities menentukan kelompok object.
- PostgreSQL MVP: database/schema/table/column, keys/constraints/index, view/materialized view, sequence, function/procedure/trigger, enum/custom type.
- MySQL MVP: database/table/column, keys/constraints/index, view, function/procedure/trigger.
- Inspector menyediakan Data, Columns, Indexes, Constraints, Foreign Keys, DDL.
- Table viewer default 100 rows dengan pagination/lazy fetch, sorting, filtering dasar, dan refresh.
- Desain Phase 2: sorting/filtering/pagination di database dengan quoted identifiers dan parameter nilai. Tanpa key unik halaman dapat berubah. Metadata terpisah tidak melihat temporary/uncommitted objects tab SQL.

## Acceptance criteria

- Metadata minimum Phase 1 sesuai §56 lulus di kedua engine; object support lengkap §10 dibuktikan sebelum Desktop MVP selesai.
- Membuka connection tidak mengambil seluruh katalog; table viewer tidak memuat seluruh tabel.
- Target 10.000+ objects diukur pada benchmark UI, bukan dianggap terbukti dari static check.

## Batasan & keputusan lanjutan

Advanced schema diff/migration dan ER diagram berada pada future scope.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Foundation](../design-docs/2026-10-09-foundation.md) dan [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md); metadata lengkap/DDL telah diuji pada adapter kedua engine. PostgreSQL table DDL adalah rekonstruksi inspeksi, bukan backup.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Bukti Foundation berbeda dari validasi desktop dan tidak menutup acceptance seluruh platform.


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.
