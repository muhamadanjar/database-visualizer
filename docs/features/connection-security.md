# Connection Manager & Credential Security

**Status implementasi:** Phase 1 koneksi runtime tervalidasi; penyimpanan credential desktop/web belum dimulai  
**Fase:** Phase 1: koneksi runtime; Phase 2: desktop persistence; Phase 3: web persistence  
**Sumber:** [PRD](../../PRD.md) §§11–12, 24, 27, 30, 39, 41.

## Tujuan

Mengelola koneksi pengguna secara aman, mulai dari test connection hingga reconnect.

## Perilaku & cakupan

- Field minimum: nama, engine, host, port, username, password, database, SSL mode; SSH tunnel dan timeout mengikuti scope/desain fase terkait.
- Foundation menyediakan connect/test/disconnect/reconnect runtime; desktop/web menambahkan create/edit/delete dan persistence.
- Desktop: password di OS credential storage; SQLite hanya metadata dan secret reference.
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
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.
