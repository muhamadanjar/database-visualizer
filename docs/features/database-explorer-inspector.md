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
- Inspector menyediakan Data, Columns, Indexes, Constraints, Foreign Keys, dan DDL sebagai viewer.
- Structure menyediakan pengelolaan schema desktop berbasis operasi terstruktur: create/rename/drop table; add/alter/rename/drop column; create/drop index; add/drop primary, unique, foreign-key, dan check constraint. SQL dialect-specific ditampilkan sebagai preview; Preview tidak menulis. Apply destructive meminta nama object persis dan konfirmasi.
- Tab Data menyediakan inline row editing melalui draft, Preview, Apply, dan readback untuk tabel transactional ber-primary key saat transport mengizinkan execute; tabel tanpa PK, view, hasil query arbitrer, dan kolom unsupported tetap read-only. Pada desktop, snapshot dan mutation terikat ke revisi profil koneksi. Setelah Apply, kembali ke Data otomatis membaca ulang halaman tabel.
- Pengelolaan schema tidak menerima SQL bebas; gunakan SQL Editor untuk DDL manual. Apply memakai koneksi terpisah dan terikat ke revisi profil saat Preview. Alter Column ditolak bila atribut/collation existing tidak dapat dipertahankan oleh form (termasuk atribut MySQL dan explicit collation PostgreSQL). UI tidak menjanjikan rollback/online DDL lintas engine; outcome tidak pasti tidak boleh diulang tanpa inspeksi schema.
- PostGIS map viewer belum tersedia.

## Acceptance criteria

- Metadata minimum Phase 1 sesuai §56 lulus di kedua engine; object support lengkap §10 dibuktikan sebelum Desktop MVP selesai.
- Membuka connection tidak mengambil seluruh katalog; table viewer tidak memuat seluruh tabel.
- Target 10.000+ objects diukur pada benchmark UI, bukan dianggap terbukti dari static check.

## Batasan & keputusan lanjutan

Advanced schema diff/migration, pengelolaan database/schema/privilege, dan ER diagram berada pada future scope. DDL terstruktur hanya menargetkan base table; viewer khusus PostGIS juga belum tersedia.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Foundation](../design-docs/2026-10-09-foundation.md), [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md), [schema change](../design-docs/2026-11-09-schema-change-management.md), dan [inline editing UX](../design-docs/2026-11-09-grid-editing-ux.md); PostgreSQL table DDL adalah rekonstruksi inspeksi, bukan backup.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Bukti Foundation berbeda dari validasi desktop dan tidak menutup acceptance seluruh platform.

## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.

2026-11-08: tombol Refresh pada Inspector Data membaca ulang metadata dan halaman data; alur UPDATE → Refresh → nilai baru lulus native Linux pada kedua engine. Gate katalog besar dan platform lain tetap terbuka pada [laporan terbaru](../plans/phase-2-desktop-mvp-validation.md).
