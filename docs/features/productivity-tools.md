# Productivity Tools

**Status implementasi:** Belum dimulai  
**Fase:** Phase 4; baseline tersedia pada MVP  
**Sumber:** [PRD](../../PRD.md) §§16, 28–29, 36, 51.

## Tujuan

Meningkatkan kecepatan pekerjaan SQL melalui autocomplete, query tersimpan, history search, dan editing.

## Perilaku & cakupan

- Tingkatkan autocomplete berdasarkan dialect/context; baseline keyword/table/column completion tetap MVP.
- Perbaiki saved query management, history search, DDL viewer, dan pengalaman basic EXPLAIN yang sudah tersedia.
- Tambahkan table editor/data editing dengan permission, transaction, serta penanganan kegagalan yang didesain sebelum implementasi.
- Lengkapi shortcuts lintas desktop/web tanpa merusak state session atau tab.

## Acceptance criteria

- Setiap peningkatan mempunyai skenario pengguna dan regression tests pada platform terkait.
- Data editing hanya memakai connection yang diotorisasi; kegagalan dan rollback terdefinisi.
- Saved query web tetap scoped workspace dan SQL tidak berubah otomatis.

## Batasan & keputusan lanjutan

Cakupan editing/concurrency dan kualitas autocomplete belum ditetapkan; advanced schema migration tetap Phase 5.

## Dokumen terkait

- [Plan utama](../plans/phase-4-productivity.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.
