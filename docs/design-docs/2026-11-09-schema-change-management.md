# Schema Change Management Design Doc

|                       |                                                                                                                                                                |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Status**            | Approved                                                                                                                        |
| **Author(s)**         | [perlu konfirmasi: author]                                                                                                       |
| **Reviewer(s)**       | Pengguna; persetujuan cakupan desain melalui percakapan 2026-11-09                                                               |
| **Tanggal Dibuat**    | 2026-11-09                                                                                                                                                     |
| **Terakhir Diupdate** | 2026-11-09                                                                                                                                                     |
| **Service Terkait**   | `crates/db-types`, `crates/db-core`, `crates/db-postgres`, `crates/db-mysql`, `packages/shared-types`, `packages/ui`, `apps/desktop`, `crates/desktop-runtime` |
| **Ticket / Issue**    | [perlu konfirmasi: issue terkait, jika ada]                                                                                                                    |

## Ringkasan (TL;DR)

Inspector saat ini menampilkan definisi DDL tetapi tidak menyediakan UI pengelolaan struktur. Proposal ini menambah pengelolaan DDL struktur tabel melalui form: create/rename/drop table; add/alter/rename/drop column; index; serta primary/unique/foreign/check constraints. Setiap perubahan menampilkan SQL spesifik engine sebelum Apply; operasi destruktif memerlukan konfirmasi dengan mengetik nama objek. PostgreSQL dan MySQL ditargetkan, dengan kemampuan yang dibatasi dan dinyatakan per engine.

## Latar Belakang & Masalah

Inspector menyediakan tab DDL sebagai viewer. Pengguna belum dapat menambah kolom atau mengubah kolom dari UI struktur. SQL Editor menjadi sarana manual untuk mengeksekusi SQL, tetapi tidak menyediakan form schema management khusus.

Perubahan schema customer berisiko mengunci tabel, gagal karena data/constraint/dependensi, dan pada engine tertentu dapat melakukan implicit commit. Karena itu, generated SQL dan batas operasi harus eksplisit; UI tidak boleh menjanjikan rollback atau reversibility yang tidak dijamin engine.

## Tujuan

- Membuat, mengganti nama, dan menghapus base table.
- Menambah, mengubah, mengganti nama, dan menghapus column.
- Membuat/menghapus index dan menambah/menghapus primary key, unique, foreign key, serta check constraints.
- Menampilkan SQL DDL persis beserta target dan dampak yang diketahui sebelum Apply; operasi yang dapat menghapus object/data mensyaratkan ketikan nama object untuk konfirmasi.
- Menampilkan hasil database apa adanya secara aman: sukses, error, atau outcome tidak pasti; refresh metadata setelah sukses.
- Menghormati authorization/execute permission dan hak database customer; UI tidak melewati kebijakan koneksi.

## Non-Tujuan (Out of Scope)

- SQL DDL bebas, migration planner, perubahan database/schema/user/privilege, view, trigger, routine, partitioning, atau opsi vendor khusus yang belum dinyatakan capability.
- Rollback otomatis, replay, atau klaim transactional/online DDL seragam lintas engine.
- Mengubah schema customer otomatis saat upgrade aplikasi.
- Menjamin zero downtime atau tidak adanya lock/rewrite.

## Proposal / Desain

### Alur dan perilaku

1. Dari Inspector struktur, pengguna memilih operasi table/column/index/constraint yang dinyatakan tersedia oleh capability engine.
2. Form memvalidasi identifier, tipe/opsi yang diketahui adapter, referensi kolom, nullability/default, serta relasi antar-object.
3. Host/adaptor menyusun SQL DDL spesifik dialect. UI menampilkan target, efek yang diketahui, dan SQL persis yang akan dieksekusi. Tidak ada perubahan schema sebelum pengguna menyetujui Apply.
4. Operasi destruktif atau berpotensi kehilangan data meminta pengguna mengetik nama object yang terdampak, selain konfirmasi Apply.
5. Apply mengirim satu operasi DDL tervalidasi dan menampilkan hasil/error database. Tidak ada retry otomatis. Jika hasil commit tidak dapat dipastikan, status dilaporkan Unknown dan pengguna diminta memeriksa schema sebelum mencoba ulang.
6. Setelah sukses terkonfirmasi, refresh metadata/DDL dari database.
7. Operation yang tidak didukung engine atau tidak dapat dipastikan aman dinonaktifkan, bukan dikirim sebagai SQL generik.

### Batas dukungan

- Mendukung PostgreSQL dan MySQL, mengikuti perbedaan sintaks/kemampuan; capability menyatakan operation per adapter.
- Operasi awal mencakup create/rename/drop table; add/alter/rename/drop column; create/drop index; add/drop primary, unique, foreign-key, dan check constraints.
- Form create/alter column mendukung tipe yang dinyatakan adapter, nullability, dan default. SQL preview wajib menunjukkan konversi tipe dan nilai default persis.
- Index awal single/composite kolom; method/index options dan expression/partial index hanya tersedia jika capability spesifiknya dinyatakan dan didesain, bukan fallback generik.
- Schema changes tidak dianggap transaksional seragam. MySQL dapat melakukan implicit commit; UI tidak menjanjikan rollback setelah Apply.

### Modul dan struktur

- `crates/db-types`/`crates/db-core` menetapkan capability dan request/response terstruktur; jangan menerima SQL bebas pada mutation API.
- `crates/db-postgres` dan `crates/db-mysql` menghasilkan SQL per dialect, quote identifier, dan menyatakan operasi yang didukung.
- `packages/shared-types` membawa DTO yang lossless; `packages/ui` menyediakan form, preview SQL, konfirmasi, state/error dan metadata refresh.
- `apps/desktop`/`crates/desktop-runtime` mengotorisasi operasi melalui session/koneksi yang ada. Tidak ada koneksi database langsung dari WebView.
- Reuse Inspector struktur; tidak menambah service/database atau migrasi schema customer.

### Perintah, gaya, dan pengujian

- Commands sesuai repository: `cargo test -p db-core -p db-types -p desktop-runtime --locked`; `pnpm test`; `pnpm typecheck`; native validation `pnpm test:native` dengan fixture PostgreSQL/MySQL disposable.
- Rust memakai rustfmt, typed errors, tanpa `unwrap()` production; TypeScript strict dua spasi/Prettier; UI memakai komponen dan transport rumah.
- Unit test generator PostgreSQL/MySQL memeriksa quoting identifier, literal/default expression policy, operation capability, dan dialect syntax.
- Integration tests pada disposable databases memeriksa table/column create-alter-rename-drop, index, constraints, dependency/data-loss failures, permission failure, dan metadata readback.
- Native E2E memverifikasi preview tidak menulis, konfirmasi nama object untuk destructive change, Apply, error, state Unknown, dan metadata refresh di kedua engine.

### Batasan implementasi

- Validasi izin wajib pada host/backend dan tetap bergantung pada DB privilege; form tidak menganggap peran aplikasi menggantikan privilege database.
- Form tidak menjalankan DDL sebelum Apply dan tidak menyimpan SQL/default plaintext ke history/log.
- Identifier berasal dari metadata tervalidasi atau input yang melalui validasi/quoting adapter; DDL hanya berasal dari operation allow-list.
- DDL tidak memiliki exactly-once/replay. Retry setelah timeout/outcome tidak pasti dilarang sampai pengguna memeriksa schema aktual.
- SQL DDL tidak dianggap transaksional seragam. Untuk MySQL khususnya, operasi dapat melakukan implicit commit; tidak menjanjikan rollback setelah Apply.

## Alternatif yang Dipertimbangkan

| Alternatif                                                               | Kelebihan                                                      | Kekurangan                                                                             | Kenapa tidak dipilih                                                  |
| ------------------------------------------------------------------------ | -------------------------------------------------------------- | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Hanya SQL Editor manual                                                  | Fleksibel dan tidak memerlukan form schema baru.               | Tidak memberi operasi terstruktur, guard destructive, atau capability per engine.      | Tidak memenuhi pengelolaan struktur dari Inspector.                   |
| Form tanpa SQL preview                                                   | Interaksi cepat.                                               | SQL vendor dan dampak operasi tidak terlihat sebelum perubahan.                        | Tidak cukup transparan untuk schema customer.                         |
| Form terstruktur + preview + typed confirmation untuk operasi destruktif | Mendukung operasi umum dengan SQL review dan guard yang jelas. | Banyak perbedaan engine dan operasi dapat lock, kehilangan data, atau implicit commit. | Dipilih; implementasi hanya mengaktifkan capability yang tervalidasi. |

## Dampak & Risiko

- **Backward compatibility**: kontrak additive; SQL editor/DDL viewer tetap berfungsi. Caller UI/transport disinkronkan dengan DTO/capability baru.
- **Migrasi data**: tidak ada migrasi schema aplikasi atau customer; DDL customer hanya berjalan setelah tindakan eksplisit.
- **Performa**: ALTER/CREATE INDEX dapat scan/rewrite/lock table dan memakai ruang; preview harus menunjukkan risiko tanpa menjanjikan online.
- **Keamanan**: privilege customer tetap wajib; identifiers di-quote dan operations allow-listed. Drop/rename/type conversion dapat merusak data/dependensi; destructive Apply meminta nama object.
- **Operasional/rollback**: perubahan dapat irreversible/implicit commit. Tidak ada rollback otomatis; timeout atau koneksi putus dapat menghasilkan Unknown dan inspeksi manual.

## Rencana Implementasi (opsional)

1. Tetapkan capability/type matrix dan DTO PostgreSQL/MySQL.
2. Implementasikan generator/validator per adapter dan test setiap operation.
3. Tambahkan form, SQL preview, typed confirmation untuk destructive Apply, state/error dan refresh Inspector.
4. Jalankan native E2E kedua engine dan dokumentasikan operasi/risiko yang didukung.

## Pertanyaan Terbuka

- Pasangan alter tipe apa yang didukung tanpa conversion policy berbahaya? Tentukan per engine sebelum kontrak capability final.
- Apakah default boleh memakai ekspresi DB-native, dan jika ya, apa allow-list/kontrak inputnya? SQL bebas penuh tetap bukan bagian endpoint mutation.
- Bagaimana confirmation mengenali lock/rewrite; bukti vendor/versi apa cukup untuk memperlihatkan estimasi?
- Apa aturan saat session berada dalam transaksi aktif, terutama ketika MySQL dapat melakukan implicit commit?
- Scope awal desktop; dukungan web memerlukan authorization/API review tersendiri.

## Referensi

- [Capability map](../../CAPABILITY-MAP.md)
- [Desktop MVP design](2026-10-09-desktop-mvp.md) — DDL viewer dan SQL editor baseline.
- [Productivity design](2026-10-10-productivity.md) — row editing dan batas schema mutation.
- [Phase 2 plan](../plans/phase-2-desktop-mvp.md)
- [Phase 4 plan](../plans/phase-4-productivity.md)
- [SQL editor & result grid](../features/sql-editor-result-grid.md)
