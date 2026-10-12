# Phase 4 — Kontrak implementasi

**Tanggal:** 2026-10-11. Implementasi mengikuti Q1–Q21 dan instruksi pengguna “implementasikan”. Artefak ini mencatat kontrak konkret; bukan bukti acceptance/release.

## API dan ownership

[OpenAPI 1.1.0](../../services/api/openapi.json) menambah POST `connections/{id}/documents/search`, PUT `documents/{id}/favorite`, POST `edits/snapshot`, POST `edits/preview`, POST `edits/readback`, DELETE `edits/{tab_id}`, serta POST/GET `mutations` dan GET/DELETE `mutations/{id}` dalam scope organisasi/workspace.

Mutator memerlukan CSRF/Origin; editing memerlukan role execute dan `X-Studio-Tab`. Snapshot terikat organisasi/workspace, user, login, browser tab, SQL tab dan koneksi; desktop memakai workspace lokal/tab. Token UUID hanya referensi snapshot ciphertext, bukan original row dari client. Preview SQL memakai placeholders; batch value dibatasi 1 MiB/100 perubahan. Snapshot 100 row/halaman diurutkan PK, offset maksimal 100.000.

Search memakai cursor AEAD berisi scope/filter, watermark dan posisi `(created_at_ms,id)`, berlaku 10 menit. Backend mengembalikan headers saja; SQL dibaca saat dokumen dipilih. Maksimal 1.000 dokumen/10 MiB/2 detik scan, 100 hasil; `complete=false` dan cursor membedakan scan parsial. Favorit private per user; saved-query revisions/ownership existing dipertahankan.

## Mutation dan transaksi

Idempotency-Key disimpan bersama HMAC request, binding koneksi/user/login/browser tab dan state; SQLite fingerprint juga encrypted. SQL/row payload tidak disimpan pada mutation record atau audit. State: `Applying`, `Applied`, `Conflict`, `Failed`, `Unknown`. Admission asynchronous desktop/web, status lookup hanya GET/IPC; respons hilang tidak memicu repost SQL. Cancellation membatalkan session khusus, termasuk request sebelum session siap.

Snapshot encrypted ber-TTL 10 menit; quota 8 MiB/tab, 32 MiB/user, 128 MiB/instance, reject tanpa menggusur snapshot user lain. Apply memeriksa revision/metadata, BEGIN, lock/reread original row, mutation, COMMIT; deadline connect/metadata/transaksi 30 detik sejak job admission, dengan rollback/cleanup bounded terpisah. Conflict/error rollback seluruh batch. COMMIT atau rollback yang tidak terkonfirmasi menghasilkan Unknown. Restart mengubah Applying ke Unknown tanpa replay. Status Applying lebih dari dua menit dibaca sebagai Unknown jika write status gagal. Tombstone terminal disimpan 30 hari; cleanup maksimal 1.000 record per langkah.

## Metadata, types dan batas

PK composite diurutkan; PK existing/generated/identity/auto-increment tidak dapat diubah. NULL, DEFAULT dan empty text berbeda. Typed integer/unsigned/decimal, finite float, boolean, text, binary base64, JSON dan local temporal tersedia; tipe opaque/timezone unsupported read-only. Decimal snapshot dibandingkan sebagai nilai exact, tanpa float conversion. Snapshot dan row-lock query memakai prepared parameters dan projection kolom yang diverifikasi.

MySQL hanya InnoDB tanpa trigger yang dependency transactional-nya belum dapat diverifikasi; table tersebut fail closed/read-only. PostgreSQL constraints/RLS/triggers tetap diberlakukan; efek trigger eksternal tidak dijanjikan atomic. Sebelum COMMIT, host membaca ulang row yang dimutasi melalui PK; generated MySQL memakai LAST_INSERT_ID pada session khusus. Setelah COMMIT terkonfirmasi, receipt encrypted sementara dapat dibaca lewat `edits/readback`, termasuk generated row di luar halaman. Receipt menggantikan original snapshot dalam quota yang sama, tanpa persistence; UI menampilkan readback lalu refresh halaman. Readback yang kedaluwarsa/tidak tersimpan tidak mengubah Applied menjadi Unknown.

## Storage dan compatibility

Migration v2 menambah favorit dan metadata mutations pada [PostgreSQL](../../services/api/migrations/0002_productivity.sql) dan SQLite; tidak memigrasikan customer schema. SQLite membuat backup private sebelum upgrade. PostgreSQL memerlukan backup operator dan API berhenti; migration transaksi memakai instance/advisory guard. Binary schema v1 menolak v2; recovery memakai binary kompatibel atau restore backup beserta key, tanpa mengubah version manual.

## Validasi terkait

[Laporan Phase 4](../plans/phase-4-productivity-validation.md) memisahkan source/unit, DB/HTTP, benchmark, browser/native, migration/recovery dan pipeline. Matrix temporal/collation/trigger/RLS, fault injection commit, stress quota dan review release tetap perlu bukti tambahan.
