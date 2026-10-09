# Query Execution, Session, Streaming & Cancellation

**Status implementasi:** Core Phase 1 tervalidasi; transport/UI Phase 2–3 belum dimulai  
**Fase:** Phase 1: core; Phase 2–3: transport/UI  
**Sumber:** [PRD](../../PRD.md) §§17, 19–22, 34–35, 37, 40–42.

## Tujuan

Menjalankan SQL secara asynchronous dengan session yang konsisten dan hasil bertahap.

## Perilaku & cakupan

- Satu session memegang koneksi tetap dengan maksimal satu query aktif; paralelisme memakai session berbeda.
- Transaksi, temporary table, dan pengaturan session tidak berpindah ke koneksi acak per statement.
- Lifecycle: Created → Running → Streaming → Completed; failure/cancellation menuju status terminal sesuai kontrak final.
- Event minimum: query_started, columns, rows, query_completed, query_failed, query_cancelled; bentuk tipe/serialisasi belum final.
- Rows dikirim per batch dengan buffer terbatas dan urutan event konsisten; jangan menunggu seluruh hasil terkumpul.
- Cancel meminta database/driver menghentikan query. Tampilkan auto commit, Commit/Rollback pada UI.
- SQL pengguna tidak dimodifikasi otomatis dan hanya dijalankan pada koneksi yang dipilih serta diotorisasi.

## Acceptance criteria

- Integration tests membuktikan session persistence, maksimal satu query aktif, streaming sebelum completion, dan cancellation nyata.
- Slow/disconnected consumer serta cleanup mengikuti kebijakan resource yang diputuskan sebelum implementasi.
- UI dan transport memetakan error/status tanpa membocorkan stack trace atau secret.

## Batasan & keputusan lanjutan

Kontrak serta default limits telah ditetapkan pada desain dan implementasi Foundation. Metadata/control menggunakan koneksi terpisah; cancel dikonfirmasi event terminal, cleanup deadline 5 detik, dan status transaksi Unknown. Schema mencantumkan results_complete untuk hasil parsial. Batas queue adalah payload aplikasi, bukan hard RSS ceiling; lihat README dan evidence plan.

## Dokumen terkait

- [Plan utama](../plans/phase-1-foundation.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.
