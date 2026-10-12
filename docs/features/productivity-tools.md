# Productivity Tools

**Status implementasi:** Kode tersedia; Phase 4 Berjalan dengan desain Approved. Acceptance/release terbuka.
**Fase:** Phase 4; baseline tersedia pada MVP.
**Sumber:** [PRD](../../PRD.md) §§16, 28–29, 36, 51.

## Tujuan

Mempercepat pekerjaan SQL desktop/web melalui completion kontekstual, pencarian dokumen, favorit pribadi, navigasi dan table editing yang aman terhadap konflik/lifecycle.

## Perilaku & cakupan

- Productivity dahulu, editing berikutnya; shared core/UI dan validasi per transport.
- Completion keyword/dialect/schema/table/column, alias FROM/JOIN dan qualified names untuk konteks umum; metadata on-demand, cache/saran dibatasi, tanpa SQL rewrite.
- Pencarian nama/SQL oleh host/backend dengan dekripsi bertahap, filter/pagination/continuation dan incomplete status. SQL tetap encrypted at rest; tidak ada plaintext search index atau dump koleksi ke browser.
- Saved queries tetap shared workspace dengan ownership/revisions, favorit pribadi; history tetap pribadi/default OFF.
- Navigasi/search/copy DDL/EXPLAIN biasa serta shortcuts sesuai fokus/izin; tidak menjalankan ANALYZE otomatis.
- CRUD grid hanya base table transactional PostgreSQL/InnoDB ber-PK/composite PK; view/no-PK/arbitrary SQL result read-only.
- Typed values/NULL/DEFAULT, generated/unsupported columns read-only; preview/confirm, scoped original snapshot, lock/reread/conflict dan dedicated atomic editing transaction.
- Draft sementara tanpa restore/autosave mutation; idempotency tanpa replay, Unknown saat commit tidak pasti, cleanup pada expiry/logout/revoke.
- Execute role existing dan privilege customer DB berlaku; Viewer metadata-only. FK/trigger tetap berlaku tanpa janji external atomicity.

## Acceptance criteria

Perilaku dan batas numeric mengikuti [desain Q1–Q21](../design-docs/2026-10-10-productivity.md); batas yang dipilih bukan hasil benchmark. Typed mutation, row identity, conflicts dan lifecycle harus lulus integrasi kedua engine serta browser/native. Search/cache/batch quotas, warm completion ≤200 ms, migration/restore, security dan regresi mempunyai bukti terpisah.

## Batasan & keputusan lanjutan

Instruksi implementasi pengguna dicatat 2026-10-11. Kontrak/schema v2 tersedia; browser/native, matrix types/trigger/privileges, fault injection commit, stress quotas, PostgreSQL restore dan reviewer release masih memerlukan bukti. Language server penuh, schema diff/migration customer, ER diagram, folders/tags shared serta custom keymap berada di luar cakupan. Gate release Phase 2–3 tetap berlaku.

## Dokumen terkait

- [Productivity — Approved](../design-docs/2026-10-10-productivity.md), [checklist Phase 4](../plans/phase-4-productivity.md).
- [Editor/grid](sql-editor-result-grid.md), [Foundation](../design-docs/2026-10-09-foundation.md), [Desktop](../design-docs/2026-10-09-desktop-mvp.md), [Web](../design-docs/2026-10-10-web-mvp.md).

## Bukti — 2026-10-10

Pengguna menyetujui Q1–Q21 dan rangkuman/izin penulisan. Recon source/graph serta template/link/diff checks membuktikan dokumentasi; tidak membuktikan editing, mutation safety, runtime atau acceptance.

## Bukti implementasi — 2026-10-11

Shared core/UI dan kedua transport menyediakan completion, bounded search/filter/favorites, DDL navigation serta staged atomic CRUD dengan idempotency/status/cancel. Generated/default/NULL/precision, conflict/rollback dan scoped access lulus fixture kedua engine. SQLite upgrade/restore lulus. Detail source/tests, benchmark fungsi completion dan gate browser/native/pipeline ada pada [laporan validasi](../plans/phase-4-productivity-validation.md). MySQL trigger dependency yang belum terverifikasi ditolak.
