# Phase 2 — Desktop MVP

**Status:** Berjalan
**Dependensi:** seluruh gate [Phase 1](phase-1-foundation.md) lulus.
**Desain:** [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md); pengguna menyetujui implementasi dan patch minimal Q15. Bukti lokal: [laporan validasi 2026-10-10](phase-2-desktop-mvp-validation.md).

**Sumber:** PRD §§5, 10–18, 21, 28–35, 39–40, 44–45, 49, 53.
**Fitur:** [desktop](../features/desktop-workspace.md), [koneksi](../features/connection-security.md), [explorer](../features/database-explorer-inspector.md), [editor/grid](../features/sql-editor-result-grid.md), [query/session](../features/query-sessions-streaming.md).

## Milestone & checklist

- [x] Persetujuan pengguna atas desain Q1–Q18, implementasi, dan patch driver Q15 dicatat; gate platform tetap terpisah.
- [x] Spike status transaksi authoritative kedua engine: raw BEGIN/COMMIT/ROLLBACK, PostgreSQL failed transaction, cancellation/disconnect, dan MySQL implicit commit; jangan memakai counter SQLx sebagai database truth.
- [x] Review perubahan driver terpisah jika spike memerlukan patch/pergantian; pertahankan jalur dan regression tests Foundation.
- [x] Spike script dialect-aware dengan source spans/preflight, stop-first-error/cancel, identitas script/statement/result, dan DTO lossless/version handshake IPC.
- [ ] Tetapkan page/ACK byte limits dan timeout IPC; buktikan slow consumer/consumer close tidak membuat antrean tak terbatas dan terminal tetap dapat diambil.
- [x] Tetapkan format/library enkripsi spool/SQL, backend OS store, schema SQLite/migrasi/backup, serta dependency lock frontend/Tauri.
- [x] Inisialisasi Tauri 2 serta React/TypeScript; implementasikan transport bersama tanpa driver database di UI.
- [x] Implementasikan connection CRUD/revisi, test/connect/disconnect/reconnect, TLS verified default, dan secret reference; perubahan profil berlaku pada session baru.
- [x] Implementasikan session lazy per SQL tab, kuota Foundation, close confirmation, cleanup/shutdown, dan reconnect tanpa replay.
- [ ] Uji OS store tersedia/locked/missing; memory-only fallback tanpa plaintext, secret update idempotent dan orphan cleanup.
- [x] Implementasikan explorer lazy loading dan inspector Data, Columns, Indexes, Constraints, Foreign Keys, DDL.
- [x] Lengkapi object support MVP: PostgreSQL materialized views, functions/procedures, triggers, enums/custom types, serta MySQL functions/procedures/triggers; verifikasi melalui adapter.
- [x] Implementasikan table viewer dengan default 100 rows, pagination/lazy fetch, sorting, filtering dasar, dan refresh.
- [x] Integrasikan Monaco, multiple tabs, current/selected/full SQL execution, formatting dasar, keyword/table/column autocomplete, dan shortcut dasar.
- [x] Implementasikan spool terenkripsi milik host, page pull, notifikasi/ACK terbatas, crash cleanup, serta hasil parsial saat quota/disk penuh.
- [x] Terapkan cache 32 MiB/tab dan 128 MiB/aplikasi; spool 1 GiB/hasil dan 4 GiB/aplikasi; ukur total RSS terpisah.
- [x] Implementasikan result grid virtualized, NULL, selection/copy/CSV, informasi eksekusi, dan sorting/filtering rows yang sudah diterima tanpa rewrite SQL.
- [x] Implementasikan Run/Cancel, auto commit, Commit/Rollback, dan session persistence melalui core.
- [x] Simpan metadata/settings/layout/recent files di SQLite; history/restore teks default OFF dan opt-in per koneksi; saved queries eksplisit; SQL persisten terenkripsi.
- [x] Terapkan history 30 hari atau 1.000 eksekusi/koneksi, opsi hapus history lama, dan tidak auto-connect/replay saat restore.
- [ ] Uji migrasi bernomor, backup/recovery, schema-version guard, kegagalan storage, dan missing secret setelah restore; jangan downgrade otomatis.
- [x] Pertahankan text/cursor/selection dan, sebisa mungkin, undo history saat pindah tab; panel resizable.
- [x] Sediakan basic EXPLAIN dan DDL viewer sebagai baseline MVP; Phase 4 memperbaiki pengalaman pengguna.
- [ ] Jalankan Vitest/React Testing Library serta WebdriverIO Tauri native untuk alur PRD §53 PostgreSQL/MySQL pada Windows, macOS, dan Linux.
- [ ] Pisahkan automation plugin/permissions dari production build; validasi paket lokal tiap OS, tanpa klaim signing/public release.
- [ ] Ukur startup, idle memory, katalog besar, dan result grid; catat angka aktual serta kondisi benchmark.

## Acceptance criteria

Pengguna dapat launch → add/test/connect PostgreSQL atau MySQL → browse objects → inspect schema/view data → open SQL tab → execute script → receive streamed rows → cancel → use transaction → opt-in/view history. Password tidak tersimpan plaintext; SQL persisten/spool terenkripsi; tab mempertahankan state; hasil memakai bounded cache/IPC/spool. Status transaksi harus authoritative sebelum gate MVP lulus; outcome Unknown/hasil parsial ditampilkan jujur.

Target PRD: cold startup <2 detik, idle memory <200 MB, navigasi 10.000+ objects dan hasil 100.000+ rows tanpa UI freeze. Target harus diukur, bukan dianggap sudah tercapai.

## Keputusan dan gate tersisa

Keputusan Q1–Q18, accessor status protocol, enkripsi XChaCha20-Poly1305, SQLite schema 1, dependency lock, dan batas IPC telah diterapkan. SSH, signing, dan auto-update ditunda. Gate tersisa: native Windows/macOS, credential store nyata tersedia/locked/missing, recovery lintas SQLite/OS store, stress slow ACK, paket tiap OS, serta benchmark cold startup/idle memory/katalog UI.

## Validasi & progres

2026-10-10: item bertanda [x] merujuk artefak kode dan hasil pada [laporan validasi](phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E dengan database nyata. Clippy, rustfmt, TypeScript, dan formatter diperiksa. Native grid menampilkan 100.000 rows pada masing-masing engine; benchmark core membaca 10.000 object PostgreSQL.

Item yang belum dicentang telah memiliki sebagian implementasi atau bukti lokal, tetapi gate lengkap belum lulus. Credential store Linux pada mesin ini tidak tersedia; encrypted persistence diuji memakai controlled store. Pipeline dan validasi tiga OS belum dijalankan. Phase 2 belum boleh ditandai Selesai atau desain Implemented.
