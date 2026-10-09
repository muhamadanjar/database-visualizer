# Phase 2 — Desktop MVP

**Status:** Belum dimulai  
**Dependensi:** seluruh gate [Phase 1](phase-1-foundation.md) lulus.  
**Sumber:** PRD §§5, 10–18, 21, 28–35, 39–40, 44–45, 49, 53.  
**Fitur:** [desktop](../features/desktop-workspace.md), [koneksi](../features/connection-security.md), [explorer](../features/database-explorer-inspector.md), [editor/grid](../features/sql-editor-result-grid.md), [query/session](../features/query-sessions-streaming.md).

## Milestone & checklist

- [ ] Bahas dan review desain Tauri IPC, event streaming, cancellation, permission scope, OS secret storage, dan SQLite lokal.
- [ ] Inisialisasi Tauri 2 serta React/TypeScript; implementasikan transport bersama tanpa driver database di UI.
- [ ] Implementasikan connection CRUD, test/connect/disconnect/reconnect dan secret reference di SQLite; password berada di OS credential storage.
- [ ] Implementasikan explorer lazy loading dan inspector Data, Columns, Indexes, Constraints, Foreign Keys, DDL.
- [ ] Lengkapi object support MVP: PostgreSQL materialized views, functions/procedures, triggers, enums/custom types, serta MySQL functions/procedures/triggers; verifikasi melalui adapter.
- [ ] Implementasikan table viewer dengan default 100 rows, pagination/lazy fetch, sorting, filtering dasar, dan refresh.
- [ ] Integrasikan Monaco, multiple tabs, current/selected/full SQL execution, formatting dasar, keyword/table/column autocomplete, dan shortcut dasar.
- [ ] Implementasikan result grid virtualized, NULL, selection, copy cell/rows/CSV, dan informasi eksekusi.
- [ ] Implementasikan Run/Cancel, auto commit, Commit/Rollback, dan session persistence melalui core.
- [ ] Simpan metadata koneksi, settings, history, saved queries dasar, recent files, dan workspace state di SQLite tanpa plaintext secrets.
- [ ] Pertahankan text/cursor/selection dan, sebisa mungkin, undo history saat pindah tab; panel resizable.
- [ ] Sediakan basic EXPLAIN dan DDL viewer sebagai baseline MVP; Phase 4 memperbaiki pengalaman pengguna.
- [ ] Jalankan frontend tests dan E2E pada PostgreSQL/MySQL untuk alur desktop PRD §53.
- [ ] Ukur startup, idle memory, katalog besar, dan result grid; catat angka aktual serta kondisi benchmark.

## Acceptance criteria

Pengguna dapat launch → add/test/connect PostgreSQL atau MySQL → browse objects → inspect schema/view data → open SQL tab → execute → receive streamed rows → cancel → use transaction → view history. Password tidak tersimpan plaintext; tab mempertahankan state; hasil memakai bounded memory.

Target PRD: cold startup <2 detik, idle memory <200 MB, navigasi 10.000+ objects dan hasil 100.000+ rows tanpa UI freeze. Target harus diukur, bukan dianggap sudah tercapai.

## Keputusan sebelum implementasi

Framework frontend/E2E, matriks OS, kontrak IPC, representasi secret reference, schema SQLite/migrasinya, dan kebijakan restore session belum ditetapkan. Code signing dan auto-update dapat ditunda sesuai PRD; kebijakan paket distribusi perlu desain tersendiri.

## Validasi & progres

Belum ada implementasi atau validasi. Catat artefak, langkah, hasil, serta batas browser/native/database/benchmark secara terpisah sebelum mencentang checklist.
