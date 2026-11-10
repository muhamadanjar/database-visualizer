# Inline Table Editing UX Design Doc

| | |
|---|---|
| **Status** | Approved |
| **Author(s)** | [perlu konfirmasi: author] |
| **Reviewer(s)** | Pengguna; persetujuan desain melalui percakapan 2026-11-09 |
| **Tanggal Dibuat** | 2026-11-09 |
| **Terakhir Diupdate** | 2026-11-09 |
| **Service Terkait** | `packages/ui`, `packages/shared-types`, `crates/query-runtime`, `crates/desktop-runtime`, `apps/desktop`, `services/api`, `apps/web` |
| **Ticket / Issue** | [perlu konfirmasi: issue terkait, jika ada] |

## Ringkasan (TL;DR)

CRUD tabel sudah tersedia sebagai tab `Edit` terpisah dengan draft, Preview, dan Apply. Proposal ini memindahkan akses pengeditan ke grid Data agar pengguna bisa mengubah sel, menambah, dan menandai baris untuk dihapus langsung dari konteks tabel, sambil tetap memakai alur staged Preview/Apply dan runtime aman yang ada. Operasi tulis per aksi tanpa review batch tidak termasuk.

## Latar Belakang & Masalah

Inspector saat ini memunculkan tab `Edit` hanya jika transport mengizinkan execute dan metadata menyatakan tabel transactional serta memiliki primary key. Tab itu menampilkan editor berhalaman 100 baris dengan kontrol tambah/hapus, draft, Preview, dan Apply. Pengguna yang mencari pengeditan langsung dari tampilan Data tidak menemukan kontrol tersebut; operasi Data dasar tetap read-only.

Backend dan UI `TableEditor` telah memiliki snapshot, typed draft, update/insert/delete, Preview/Apply, batas batch, konflik, status Unknown, dan refresh/readback. Karena itu kebutuhan UX ini harus menggunakan ulang kemampuan yang ada, bukan membangun jalur mutation kedua.

## Tujuan

- Dari tab Data, pengguna dapat memasuki mode edit untuk tabel yang memenuhi syarat, mengubah nilai sel, menambah baris dan menandai baris untuk dihapus.
- Perubahan tetap draft sampai pengguna melakukan Preview lalu Apply eksplisit; transaksi database, conflict detection, batas resource, dan penanganan Unknown yang ada tetap berlaku.
- Tabel tanpa PK, object non-transactional, view, dan hasil query arbitrer tetap read-only.
- Sukses menampilkan readback/refresh; konflik atau kegagalan tidak menyamar sebagai sukses dan tidak mengulang Apply otomatis.
- Alur edit yang tersedia sekarang tetap berfungsi selama UX baru diperkenalkan.

## Non-Tujuan (Out of Scope)

- Commit langsung per sel/baris atau bypass Preview/Apply.
- Mengedit primary key, generated/identity columns, tipe yang sudah ditandai unsupported, view, tabel tanpa PK, atau hasil SQL arbitrer.
- Mengubah protokol CRUD, persistence draft, privilege, atau transaksi khusus Phase 4.
- Mengganti behavior operasional editor tabel yang telah ada kecuali diperlukan agar bisa dipakai dari tab Data.
- PostGIS map viewer; kapabilitas itu memiliki dokumen spesifikasi terpisah.

## Proposal / Desain

### Alur UX

1. Tab Data menyediakan aksi `Edit rows` hanya ketika tabel memenuhi capability metadata dan transport mendukung execute.
2. Saat mode edit aktif, perubahan sel/add/delete masuk ke draft in-memory pada snapshot tabel yang sama; nilai PK/generated/unsupported tetap non-editable.
3. Pengguna dapat membuang draft atau membuka Preview yang merangkum perubahan. Apply memerlukan konfirmasi eksplisit dan menggunakan existing dedicated editing transaction.
4. Apply yang berhasil refresh/readback data; conflict/Failed mengarahkan pengguna refresh dan meninjau ulang; Unknown meminta pemeriksaan data dan tidak menyediakan retry buta.
5. Navigasi, penutupan tab, dan refresh mengikuti proteksi unsaved-draft yang sudah ada.

### Batas modul

- `packages/ui`: UX entry point/mode edit pada Data grid; reuse `TableEditor` dan `editDrafts` atau ekstrak hanya bila perlu agar komponen yang sama dapat dirender pada tab Data.
- Shared contracts dan Rust editing runtime tetap sumber capability, snapshot, typed mutations, preview/apply, dan status.
- Tidak ada koneksi database dari UI dan tidak ada API mutation baru kecuali implementasi existing ternyata tidak dapat mendukung entry point baru.

### Perintah dan struktur

- Build/test UI desktop: `pnpm build`, `pnpm test`, `pnpm typecheck`.
- Native integration untuk PostgreSQL dan MySQL: `pnpm test:native` dengan fixture disposable.
- UI berada di `packages/ui/src/Workspace.tsx`, `packages/ui/src/TableEditor.tsx`, serta test di `packages/ui/src/Workspace.test.tsx`; Rust mutation berada di `crates/query-runtime` dan `crates/desktop-runtime`.
- Ikuti TypeScript strict, dua spasi, Prettier, test React Testing Library/Vitest dan native WDIO yang sudah digunakan.

### Batasan dan keamanan

- Aksi tulis hanya pada base table transactional dengan PK/composite PK dan capability yang diberikan metadata tepercaya.
- Gunakan kembali scoped snapshots, optimistic/original-row conflict checks, parameter bertipe, preview, dedicated transaction, cancellation, idempotency/status, dan existing resource limits.
- Draft tetap memory-only. Jangan memasukkan SQL, row values, credential, atau draft ke logs/history/localStorage.

## Alternatif yang Dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Kenapa tidak dipilih |
|---|---|---|---|
| CRUD langsung per aksi dari grid | Interaksi cepat dan sedikit langkah. | Kehilangan preview batch; risiko partial writes dan overwrite konflik; menambah jalur transaksi baru. | Tidak sejalan dengan alur aman existing dan keputusan Phase 4. |
| Pertahankan tab Edit terpisah tanpa entry dari Data | Tidak mengubah UI. | Pengguna tetap tidak menemukan pengeditan dari grid yang sedang dilihat. | Tidak menyelesaikan masalah discoverability yang dilaporkan. |
| Entry/mode edit pada tab Data dengan staged Apply | Konteks tabel tetap, reuse snapshot dan mutation backend existing. | Memerlukan pemisahan UX editor dari tab khusus dan penanganan navigasi draft. | Dipilih sebagai perubahan terkecil yang memenuhi kebutuhan tanpa melemahkan write safety. |

## Dampak & Risiko

- **Backward compatibility**: kontrak transport/database tidak berubah; tab Edit lama perlu tetap berfungsi atau menjadi satu entry point yang sama.
- **Migrasi data**: N/A; tidak mengubah schema customer atau persistence aplikasi.
- **Performa**: reuse snapshot/page/budget yang sudah ada; jangan memperluas jumlah row yang dimuat demi UX inline.
- **Keamanan**: tetap memerlukan PK, transactional engine, fresh capability/authorization, DB privilege, conflict validation, dan explicit Apply. Salah mengaktifkan mode pada table/view yang tak eligible dapat menulis data yang salah.
- **Operasional/rollback**: feature UI dapat dipulihkan ke tab Edit lama tanpa perubahan migrasi; setiap Apply tetap mengikuti status dan Unknown semantics existing.

## Rencana Implementasi (opsional)

1. Validasi eligibility dan pengalaman current Edit dengan fixture kedua engine.
2. Tambahkan entry/mode staged editing dari Data grid, gunakan kembali draft dan handler existing.
3. Uji edit/update/insert/delete, preview/apply, conflict, cancel, refresh/discard, navigasi/close, dan read-only gating pada UI/native.

## Pertanyaan Terbuka

- Apakah entry point harus mengganti tab Edit yang lama atau tab itu tetap ada sebagai shortcut? Default proposal: jadikan satu implementasi editor dengan entry point dari Data dan hapus tab duplikat setelah migrasi UX terbukti.
- Grid virtualization dan cell editor keyboard/accessibility mana yang wajib pada acceptance awal?
- Acceptance desktop/native Phase 2/4 dan browser/web harus dipisahkan; runtime browser aktual masih gate terpisah.

## Referensi

- [Capability map](../../CAPABILITY-MAP.md)
- [Productivity design, editing contract](2026-10-10-productivity.md)
- [Phase 4 plan](../plans/phase-4-productivity.md)
- [Phase 4 validation](../plans/phase-4-productivity-validation.md)
- [SQL editor & result grid](../features/sql-editor-result-grid.md)
