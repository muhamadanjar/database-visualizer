# Database Explorer, Inspector & Table Viewer

**Status implementasi:** Metadata Foundation tervalidasi; UI explorer/inspector dan metadata lengkap MVP belum dimulai  
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

## Acceptance criteria

- Metadata minimum Phase 1 sesuai §56 lulus di kedua engine; object support lengkap §10 dibuktikan sebelum Desktop MVP selesai.
- Membuka connection tidak mengambil seluruh katalog; table viewer tidak memuat seluruh tabel.
- Target 10.000+ objects diukur pada benchmark UI, bukan dianggap terbukti dari static check.

## Batasan & keputusan lanjutan

Advanced schema diff/migration dan ER diagram berada pada future scope.

## Dokumen terkait

- [Plan utama](../plans/phase-2-desktop-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.
