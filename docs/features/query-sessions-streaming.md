# Query Execution, Session, Streaming & Cancellation

**Status implementasi:** Foundation dan transport/host/UI desktop tervalidasi lokal; stress ACK native dan web masih terbuka
**Fase:** Phase 1: core; Phase 2–3: transport/UI
**Sumber:** [PRD](../../PRD.md) §§17, 19–22, 34–35, 37, 40–42.

## Tujuan

Menjalankan SQL secara asynchronous dengan session yang konsisten dan hasil bertahap.

## Perilaku & cakupan

- Satu session memegang koneksi tetap dengan maksimal satu query aktif; paralelisme memakai session berbeda.
- Transaksi, temporary table, dan pengaturan session tidak berpindah ke koneksi acak per statement.
- Lifecycle: Created → Running → Streaming → Completed; failure/cancellation menuju status terminal sesuai kontrak final.
- Event Foundation: query_started, columns, rows, query_completed, query_failed, query_cancelled; kontrak Rust tersedia, DTO/versi transport desktop dirancang pada Phase 2.
- Rows dikirim per batch dengan buffer terbatas dan urutan event konsisten; jangan menunggu seluruh hasil terkumpul.
- Cancel meminta database/driver menghentikan query. Tampilkan auto commit, Commit/Rollback pada UI.
- SQL pengguna tidak dimodifikasi otomatis dan hanya dijalankan pada koneksi yang dipilih serta diotorisasi.

## Acceptance criteria

- Integration tests membuktikan session persistence, maksimal satu query aktif, streaming sebelum completion, dan cancellation nyata.
- Slow/disconnected consumer serta cleanup mengikuti kebijakan resource yang diputuskan sebelum implementasi.
- UI dan transport memetakan error/status tanpa membocorkan stack trace atau secret.

## Batasan & keputusan lanjutan

Kontrak serta default limits telah ditetapkan pada desain dan implementasi Foundation. Metadata/control menggunakan koneksi terpisah; cancel dikonfirmasi event terminal, cleanup deadline 5 detik, dan status transaksi Unknown bila session busy/invalid; Phase 2 menambahkan Idle/Active/Failed berdasarkan protocol engine. Schema mencantumkan results_complete untuk hasil parsial. Batas queue adalah payload aplikasi, bukan hard RSS ceiling; lihat README dan evidence plan.

Desain Phase 2 menambah script berurutan dan versi IPC tanpa menghapus jalur satu-statement. Auto Commit OFF membuka transaksi ketika Idle terkonfirmasi; perubahan mode meminta penyelesaian transaksi aktif. Status authoritative telah dibuktikan melalui accessor protocol minimal yang disetujui, bukan SQL/counter SQLx. Host spool/page pull dan ACK terbatas melanjutkan backpressure ke desktop; telah diterapkan; stress ACK native belum lengkap.

## Dokumen terkait

- [Plan utama](../plans/phase-1-foundation.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Foundation](../design-docs/2026-10-09-foundation.md) dan [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md).

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Bukti Foundation berbeda dari validasi desktop dan tidak menutup acceptance seluruh platform.


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.


## Implementasi web Phase 3 — Berjalan

[Web MVP](../design-docs/2026-10-10-web-mvp.md) mempertahankan session tetap pribadi per user/tab dan script/transaksi core. HTTP creation idempotent; WebSocket lifecycle/requested pages memakai satu ACK credit, 256 rows/2 MiB dan timeout 30 detik; HTTP terminal tetap tersedia.

Reconnect 30 detik tanpa replay, hasil selesai sementara 10 menit, revoke/logout/expiry tanpa grace. Session/spool quotas berlaku pada user/workspace/instance; resource authorization juga memeriksa pemilik. Shared runtime diekstrak bertahap dengan regresi desktop. Kontrak final, crash/idempotency, slow-consumer dan browser E2E masih gate [Phase 3](../plans/phase-3-web-mvp.md).

2026-10-10: kode Phase 3 tersedia; [laporan validasi](../plans/phase-3-web-mvp-validation.md) memisahkan unit/static dari gate API/browser/DNS/TLS/Docker. [Runbook](../plans/phase-3-web-mvp-operations.md) menjelaskan konfigurasi operator. Patch TCP Q15 telah disetujui; hostname TLS dipertahankan dan IP tervalidasi dipin, tetapi runtime TLS/egress belum dibuktikan.
