# Phase 4 — Productivity

**Status:** Belum dimulai  
**Dependensi:** [Desktop MVP](phase-2-desktop-mvp.md) dan [Web MVP](phase-3-web-mvp.md) tervalidasi.  
**Sumber:** PRD §§16, 28–29, 36, 51.  
**Fitur:** [productivity](../features/productivity-tools.md), [editor/grid](../features/sql-editor-result-grid.md).

## Milestone & checklist

- [ ] Evaluasi baseline autocomplete, saved queries, EXPLAIN, history, dan DDL viewer yang telah diwajibkan MVP.
- [ ] Bahas/review desain autocomplete lanjutan dan data editing beserta permission, transaksi, conflict handling, dan query safety.
- [ ] Tingkatkan autocomplete sesuai dialect dan context tanpa memaksakan fitur engine yang tidak didukung.
- [ ] Tingkatkan pengelolaan saved queries, history search, dan navigasi DDL/EXPLAIN.
- [ ] Tambahkan table editor/data editing dengan preview/konfirmasi perubahan sesuai desain yang disetujui.
- [ ] Lengkapi keyboard shortcuts dan dokumentasikan perilakunya di desktop/web.
- [ ] Uji regresi session, tenant authorization, query execution/cancel, serta alur editing pada kedua engine.

## Acceptance criteria

Fitur tambahan dapat dipakai melalui core dan transport yang sama. Saved query web tetap scoped workspace; data editing mematuhi permission dan transaksi serta memiliki perilaku kegagalan yang terdokumentasi. Peningkatan autocomplete tidak mengubah SQL yang dijalankan tanpa tindakan pengguna.

Basic autocomplete, saved query CRUD, EXPLAIN, DDL inspection, dan shortcuts sudah merupakan baseline MVP; fase ini meningkatkan pengalaman, bukan alasan menundanya.

## Keputusan sebelum implementasi

Cakupan editing, strategi deteksi perubahan bersamaan, recovery/rollback, dan ukuran metadata cache belum ditentukan. Detail UI, API tambahan, serta kriteria kualitas autocomplete perlu dibahas pada desain fase ini.

## Validasi & progres

Belum ada implementasi atau validasi. Catat skenario pengguna, regresi, database integration, dan keamanan; jangan klaim data editing aman hanya dari unit test.
