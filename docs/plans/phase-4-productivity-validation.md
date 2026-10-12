# Phase 4 — Validasi implementasi

**Tanggal:** 2026-10-11. **Status:** Berjalan; kode tersedia, release belum diterima. Desain [Approved](../design-docs/2026-10-10-productivity.md) berdasarkan instruksi implementasi pengguna. Kontrak konkret: [API/storage/lifecycle](../design-docs/phase-4-productivity-contracts.md).

## Bukti lokal

- `cargo test -p query-runtime -p studio-api -p desktop-runtime -p db-core -p db-types --features studio-api/test-provider --locked -- --include-ignored`: 51 test lulus sebelum tambahan restore/cancel; regresi Foundation dan HTTP/WebSocket kedua engine berhasil.
- Suite lanjutan `cargo test -p query-runtime -p desktop-runtime -p studio-api --features studio-api/test-provider --locked -- --include-ignored`: 19 test lulus; termasuk tambahan restore SQLite, admission/status desktop, generated/default/NULL/precision, readback generated row di luar halaman pertama dan pembatalan row lock PostgreSQL/MySQL. Scope kedua suite saling overlap; jangan menjumlahkan sebagai test unik.
- `crates/query-runtime/tests/editing.rs`: kedua engine, composite PK/bigint >2^53, update/delete/insert, DEFAULT/NULL/empty, identity/auto-increment readback, rollback constraint, concurrent row conflict, token ownership/claim dan pembatalan blocked row lock.
- `crates/desktop-runtime/tests/security.rs`: search 100+5 hasil/cursor/filter binding, favorit, ciphertext tanpa SQL/request plaintext, idempotency conflict dan restart Unknown; backup schema v1, upgrade v2 dan restore encrypted documents dengan key yang sama.
- `services/api/tests/web.rs`: private favorites, shared saved-query search, headers tanpa SQL, Viewer denial, cross-user preview/status denial, mutation admission/poll/repeat tanpa eksekusi ulang receipt readback dengan cross-user denial dan reread changed row pada kedua engine; schema/instance guard dan regresi tenant/session/HTTP/WS.
- Frontend: 16 Vitest test lulus; completion alias/dialect/active statement/strings/comments/200 suggestions, metadata oversize rejection dan transport lost-response lookup tanpa repost. Typecheck, build desktop/web, Prettier dan Rust fmt lulus. Clippy paket core/adapters/runtime/API all-targets dengan `-D warnings` lulus; empat warning upstream vendor SQLx dipisahkan. Build masih memberi warning ukuran chunk Monaco.

## Benchmark lokal

`node scripts/productivity-benchmark.mjs`: Node v24.18.0, 20.000 metadata names, 100 sampel, p95 **18,41 ms**, maksimum **28,10 ms**. [Artefak pengukuran](phase-4-productivity-benchmark.json). Ini fungsi completion hangat saja, tidak termasuk Monaco render, network atau cold metadata. Target 200 ms terpenuhi pada fixture lokal; belum SLA browser/native. Jalankan ulang untuk kondisi mesin berbeda.

## Gate yang belum lulus

Binary `studio-api migrate` untuk host browser gagal membuka TCP dengan `PermissionDenied`, errno 1 dalam lingkungan managed ini. Proses test integrasi berhasil mengakses fixture DB dan HTTP; izin tersebut tidak membuktikan host aplikasi dapat berjalan. Browser Phase 4 dan native Phase 4 belum dijalankan; screenshot atau source scenario bukan bukti interaksi.

Tetap terbuka: real OAuth/TLS/egress dan gate release Phase 2–3; Windows/macOS/OS keystore nyata; browser/native CRUD/shortcut/draft flows; matrix temporal/timezone/JSON/collation, RLS/privilege/trigger dependencies; commit-response loss fault injection; stress/fairness/TTL/retention quotas; backup/restore PostgreSQL dan failure migration/rotation; cold metadata/search benchmark; pipeline aktual serta reviewer release. MySQL trigger dependencies yang belum diverifikasi ditolak, bukan dianggap aman.

## Reproduksi dan operasi

Gunakan disposable databases `docker compose up -d --wait`; jalankan command test di atas. `pnpm test`, `pnpm typecheck`, `pnpm typecheck:web`, `pnpm build`, `pnpm build:web`, `pnpm format:check`, Rust fmt dan workspace Clippy merupakan checks terpisah. Scenario `tests/web/workspace.browser.js` kini mencakup update/insert/default/NULL/delete/readback dan sudah diperiksa syntax; belum runtime proof. Browser memakai `pnpm test:web-browser` setelah API fixture/Vite berjalan; proxy dapat diubah dengan `STUDIO_API_PROXY` jika port 8080 dipakai aplikasi lain. Native memakai `pnpm test:native` pada desktop yang sesuai.

Hentikan API sebelum `studio-api migrate`, backup database aplikasi dan key versions secara terpisah. SQLite melakukan backup sebelum v2; tutup desktop sebelum restore. Jangan downgrade version manual. Unknown memerlukan verifikasi data melalui query dan review baru, bukan replay key/batch lama. Draft rows tidak disimpan ke localStorage/history/restore. Jangan mengunggah auth state, SQL, rows, credential atau operator key dari test.
