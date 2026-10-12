# Phase 4 — Productivity

**Status:** Berjalan; kode tersedia, acceptance/release terbuka.
**Dependensi:** desain boleh berjalan sekarang; acceptance/release tetap memerlukan gate [Desktop MVP](phase-2-desktop-mvp.md) dan [Web MVP](phase-3-web-mvp.md).
**Desain:** [Productivity — Approved](../design-docs/2026-10-10-productivity.md); Q1–Q21/izin penulisan 2026-10-10, persetujuan implementasi melalui instruksi pengguna 2026-10-11.
**Bukti:** [validasi Phase 4](phase-4-productivity-validation.md), [kontrak konkret](../design-docs/phase-4-productivity-contracts.md).
**Sumber:** PRD §§16, 28–29, 36, 51.
**Fitur:** [productivity](../features/productivity-tools.md), [editor/grid](../features/sql-editor-result-grid.md).

## Milestone & checklist

### Desain dan prerequisites

- [x] Recon baseline dan gaps: 2026-10-10, source/graph memverifikasi completion sederhana, dokumen berhalaman, metadata PK tekstual, parameter string dan schema v1; referensi pada desain. Bukti source, bukan runtime editing.
- [x] Grilling Q1–Q21 dan konfirmasi rangkuman/penulisan: 2026-10-10, log keputusan pada desain; bukan persetujuan implementasi.
- [x] Tulis/register Draft dan sinkronkan fitur/indeks: 2026-10-10, struktur template, tautan lokal dan `git diff --check` lulus; dokumentasi saja.
- [x] Persetujuan implementasi pengguna 2026-10-11 dicatat pada desain; kontrak konkret dan DDL tersedia. Review acceptance/release independen tetap gate terpisah.
- [x] Spike structured PK/generated/writable metadata, typed params/round-trip dan original-row conflict comparison kedua engine. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [ ] Spike generated-key readback, transactional storage/dependency detection, row locks, cancel/rollback dan unknown commit.
- [ ] Review additive OpenAPI/DTO/capabilities, snapshot/cursor integrity, idempotency/status/retention dan DDL v2 sebelum consumer.

### Milestone A — Productivity desktop/web

- [x] Shared completion untuk dialect, schema/table/column, FROM/JOIN aliases dan qualified names pada konteks umum; tanpa SQL rewrite otomatis. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [ ] On-demand metadata/cache 32 MiB/koneksi, 128 MiB/aplikasi, TTL 5 menit; 200 saran, invalidasi scope/revision/refresh/izin.
- [x] Scoped POST search nama/SQL dengan decrypt scan ≤1.000 dokumen/10 MiB/2 detik, ≤100 hasil dan continuation; incomplete status eksplisit. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Filter koneksi/tanggal/status, stable pagination; private history/default OFF dan shared saved-query ownership/revisions tetap berlaku. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Favorit pribadi, list search/filter/sort dan cleanup referensi setelah delete/revoke; tanpa folders/tags shared. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Tingkatkan navigasi/search/copy DDL/EXPLAIN biasa; tanpa ANALYZE otomatis atau customer schema mutation. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Shortcuts bersama sesuai fokus/izin dan help menu tersedia: Ctrl/⌘+Alt+S/J/H serta Alt+Shift+F. Verifikasi reserved shortcuts per OS/browser tetap gate. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.

### Milestone B — Table/data editing

- [x] Additive structured ordered PK/column/type/editability metadata dan typed mutation; text filter/query existing tetap berfungsi. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] CRUD hanya base table transactional ber-PK/composite PK; nontransactional/no-PK/view/arbitrary result read-only. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] NULL/DEFAULT/empty/precision dibedakan; generated/identity/auto-increment dan PK existing tidak diedit; unsupported types read-only. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Scoped encrypted original-row snapshots, metadata/key validation, row locking/reread/full original-value comparison; conflict rollback seluruh batch. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [x] Draft/preview/confirm, dedicated editing session dengan kuota existing, atomic batch dan refresh/readback setelah success; transaksi tab SQL tidak disentuh. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [ ] Batas 100 perubahan/1 MiB, snapshot 8 MiB/tab–32 MiB/user–128 MiB/instance, TTL 10 menit, Apply 30 detik; reject tanpa evict pengguna lain.
- [x] Durable admission/idempotency, status lookup tanpa repost; restart tidak replay; Unknown ketika commit/rollback tidak pasti. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.
- [ ] Fresh execute/tenant/tab checks, Viewer metadata-only, DB privilege/constraint/FK/trigger tetap berlaku; efek eksternal tidak dijanjikan atomic.
- [x] Close/navigation draft prompt, expiry/conflict refresh, logout/revoke cleanup, tanpa autosave/localStorage/history/restore draft rows. Bukti 2026-10-11: source dan suite terkait pada laporan validasi; browser/native tetap gate terpisah.

### Migration, operations dan acceptance

- [ ] PostgreSQL backup/restore dan failure migration/rotation; migration v2 transaksi/guard tersedia dan SQLite upgrade/restore sudah lulus (2026-10-11, laporan validasi).
- [ ] Uji upgrade/failure/restore/key recovery/compatible rollback; old binary menolak schema baru, tanpa automatic downgrade/customer migration.
- [ ] Unit/controlled tests untuk capability, typed values, identifiers, budgets/cursors, shortcuts dan lifecycle; pisahkan dari integrasi.
- [ ] PostgreSQL/MySQL integration: CRUD/composite PK, NULL/default/generated/precision, conflicts/concurrency, constraint/RLS/privilege failure dan transactional guarantees.
- [ ] HTTP/WS/privacy/revoke/idempotency/restart/cancel/unknown-commit tests; tanpa SQL/search terms/rows/secret pada audit/log.
- [ ] Browser/native flows kedua engine: productivity, preview/confirm, staged edits, conflict refresh, lifecycle dan permission; platform evidence terpisah.
- [ ] Benchmark warm completion ≤200 ms, cold/large catalog 10.000 objects, search continuation/budgets, 100.000 rows, fairness/RSS/disk/cleanup dengan kondisi terukur.
- [ ] Regresi session/query/cancel/transactions/tenant/privacy Phase 2–3 dan pipeline aktual; close gate release sebelumnya.

## Acceptance criteria

Desktop/web memakai shared contracts/core/UI; baseline query/filter tetap kompatibel. Pencarian tidak membuka private history atau plaintext persistent index. Autocomplete tidak mengubah SQL otomatis. Editing menggunakan PK asli, tipe aman, fresh authorization dan dedicated atomic transaction; conflict/unknown ditampilkan tanpa replay atau janji rollback yang belum terbukti.

Semua batas resource memiliki failure-path evidence. Runtime kedua engine, browser/native, migration/restore/benchmark/pipeline dan gate fase sebelumnya harus mempunyai bukti sebelum Phase 4 Selesai. Basic autocomplete/saved queries/EXPLAIN/DDL tetap baseline MVP; Phase 4 meningkatkan pengalaman tersebut.

## Blockers dan review

Keputusan Q1–Q21 dan implementasi disetujui pengguna. Reviewer release, browser/native Phase 4, matrix types/privileges/triggers, fault injection commit, stress budgets, PostgreSQL restore dan pipeline tetap terbuka. Host CLI browser dibatasi TCP PermissionDenied (errno 1); proses test DB/HTTP berjalan. Rincian pada laporan validasi. MySQL dengan trigger yang dependency-nya tidak terbukti transactional tetap read-only.

## Validasi & progres

2026-10-10: recon melalui graph/source, sesi grilling dan penulisan dokumen selesai. Struktur `TEMPLATE.md`, links Markdown relatif dan diff whitespace diperiksa. Tidak ada implementasi Phase 4 atau pengujian runtime baru. Gate Phase 2–3 tetap mengikuti laporan masing-masing; keterbatasan TCP/browser/OAuth/Docker tidak diselesaikan oleh dokumen ini.

2026-10-11: implementasi completion/search/favorites/navigation/editing, migration v2 dan kontrak additive tersedia. Regresi DB/HTTP/WebSocket serta unit frontend lulus; lihat laporan untuk commands, scope dan batas bukti. Phase 4 belum Selesai.
