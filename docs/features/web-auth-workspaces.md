# Web Authentication, Workspace & Tenant Isolation

**Status implementasi:** Berjalan; kode web/API tersedia, acceptance runtime belum lengkap.
**Fase:** Phase 3
**Sumber:** [PRD](../../PRD.md) §§22–29, 39, 43, 50, 53.

## Tujuan

Web client PostgreSQL/MySQL untuk beberapa organisasi/workspace dalam deployment Docker yang dikelola operator, dengan shared UI/core dan target database yang diizinkan.

## Perilaku & cakupan

- Browser → satu origin HTTPS/WebSocket → Axum/query engine satu replica → shared core → authorized customer database; control plane PostgreSQL terpisah.
- GitHub/Google login, server-side session/cookie Secure/HttpOnly/SameSite; CSRF dan WebSocket Origin checks. Login idle 30 menit/absolut 12 jam.
- Owner pertama ditentukan operator; akses membership melalui undangan. Tidak ada organisasi public self-registration atau automatic account linking berdasarkan email.
- Owner mengelola organisasi/workspaces; Admin mengelola workspace ditugaskan; Developer metadata/data/query dan saved query miliknya; Viewer metadata struktur saja.
- Resource scoped organization/workspace; session/result/history/editor juga scoped pemilik user/tab. Repository/FK/authorization backend dan negative tests menjadi security boundary.
- Credential dan SQL persisten dienkripsi backend menggunakan versioned key melalui SecretStore; browser tidak menerima kembali credential tersimpan.
- HTTP resource API berversi/OpenAPI dan creation execution idempotent; WebSocket mengirim lifecycle serta requested row pages dengan ACK/batas byte.
- Reconnect grace 30 detik tanpa SQL replay, hasil selesai sementara 10 menit. Logout/expiry/revoke memulai cleanup tanpa grace atau auto-Commit.
- History/restore default OFF dan pribadi; saved queries shared workspace dengan ownership/revision. Browser file/clipboard tidak mengakses filesystem server.

## Acceptance criteria

- Alur PRD §53 kedua engine lulus browser E2E; provider GitHub/Google nyata diuji terpisah.
- Semua protected HTTP/WS/resource/page/documents menolak lintas tenant/user atau izin yang dicabut.
- Viewer tidak bypass query permission melalui table viewer; tidak ada main auth token di localStorage.
- Duplicate/reconnect/restart tidak mengeksekusi ulang SQL; quota/slow consumer/cancel/privacy/recovery diuji.
- Deployment/operator keys, enforced egress/DNS/TLS, migrations/rollback dan benchmark punya bukti nyata.

## Batasan & keputusan lanjutan

Q1–Q20, implementasi, dan patch pinning TLS disetujui pengguna. HTTP/WS auth, scoped persistence, shared runtime dan browser transport tersedia. Login fixture hanya aktif melalui feature `test-provider` dan development mode; itu bukan OAuth nyata. DNS/TLS/egress, API/DB/browser penuh, recovery, stress dan pipeline tetap gate release. RLS, Redis, multi-replica dan Vault konkret berada di luar MVP.

## Dokumen terkait

- [Web MVP — Approved](../design-docs/2026-10-10-web-mvp.md), [plan/checklist](../plans/phase-3-web-mvp.md).
- [Foundation](../design-docs/2026-10-09-foundation.md), [Desktop MVP](../design-docs/2026-10-09-desktop-mvp.md), [bukti desktop](../plans/phase-2-desktop-mvp-validation.md).

## Bukti — 2026-10-10

[Validasi](../plans/phase-3-web-mvp-validation.md) memisahkan unit/static dari API/browser/deployment. [Runbook](../plans/phase-3-web-mvp-operations.md) menjelaskan konfigurasi lokal dan operator. Status Berjalan tidak menyatakan production ready.
