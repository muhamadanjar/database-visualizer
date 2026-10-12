# Connection Manager & Credential Security

**Status implementasi:** Runtime dan connection CRUD desktop tervalidasi lokal; OS store native belum tersedia di mesin Linux ini; web/API tersedia, gate runtime terbuka
**Fase:** Phase 1: koneksi runtime; Phase 2: desktop persistence; Phase 3: web persistence
**Sumber:** [PRD](../../PRD.md) §§11–12, 24, 27, 30, 39, 41.

## Tujuan

Mengelola koneksi pengguna secara aman, mulai dari test connection hingga reconnect.

## Perilaku & cakupan

- Field minimum: nama, engine, host, port, username, password, database, SSL mode; SSH tunnel dan timeout mengikuti scope/desain fase terkait.
- Foundation menyediakan connect/test/disconnect/reconnect runtime; desktop/web menambahkan create/edit/delete dan persistence.
- Desktop: password di OS credential storage; SQLite hanya metadata dan secret reference.
- Desain Phase 2: store tidak tersedia menawarkan memory-only tanpa plaintext fallback; SQL persisten/spool terenkripsi, TLS verified default. Profil direvisi untuk session baru; reconnect eksplisit menerapkan perubahan. Update secret memakai reference baru dan recovery idempotent; hapus profil setelah cleanup session.
- Web: secret dikelola serta dienkripsi backend; credential tersimpan tidak dikirim kembali ke browser.
- Otorisasi tenant/connection dijalankan backend. Secret tidak masuk log, repository, atau frontend storage; SQL customer terpisah dari control plane.

## Acceptance criteria

- Connection CRUD/test/connect/disconnect/reconnect dapat diverifikasi pada platform terkait.
- Error tidak membocorkan secret; pengujian web menolak akses lintas tenant.
- Metadata dapat dipulihkan tanpa menyimpan plaintext password.

## Batasan & keputusan lanjutan

Connect timeout default 10 detik; query timeout opsional. Runtime config tidak serializable, Debug redacted, dan SQL logging driver mati. OS credential/secret-store persistence serta SSH tunnel bukan pekerjaan Foundation.

## Dokumen terkait

- [Plan utama](../plans/phase-1-foundation.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Foundation](../design-docs/2026-10-09-foundation.md) dan [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md); backend OS store/enkripsi diterapkan; recovery dan OS store nyata memerlukan pengujian platform.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Bukti Foundation berbeda dari validasi desktop dan tidak menutup acceptance seluruh platform.


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.


## Implementasi web Phase 3 — Berjalan

[Web MVP](../design-docs/2026-10-10-web-mvp.md) menetapkan credentials/SQL terenkripsi backend dengan mounted versioned key dan SecretStore abstraction. Owner/Admin mengelola koneksi/secret; Developer menggunakannya tanpa mendapat kembali password. Target memakai operator allowlist host/port/network, TLS verified, dan customer-path protection dari control plane.

DNS/IP pinning yang mempertahankan TLS hostname telah diterapkan setelah review Q15; enforced egress dan DNS/TLS runtime tetap gate. Ini berbeda dari patch status protocol Phase 2. Rotation/restore key, cross-tenant rejection, dan provider/deployment nyata belum diuji web; lihat [checklist](../plans/phase-3-web-mvp.md).

2026-10-10: kode Phase 3 tersedia; [laporan validasi](../plans/phase-3-web-mvp-validation.md) memisahkan unit/static dari gate API/browser/DNS/TLS/Docker. [Runbook](../plans/phase-3-web-mvp-operations.md) menjelaskan konfigurasi operator. Patch TCP Q15 telah disetujui; hostname TLS dipertahankan dan IP tervalidasi dipin, tetapi runtime TLS/egress belum dibuktikan.
