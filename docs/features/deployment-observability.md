# Development Environment, Deployment & Observability

**Status implementasi:** Environment development tersedia; deployment/observability produksi belum dimulai
**Fase:** Phase 1: local/test; Phase 3: web deployment; distribusi desktop setelah core stabil
**Sumber:** [PRD](../../PRD.md) §§38, 40, 42–47.

## Tujuan

Menyediakan lingkungan pengembangan yang dapat direproduksi serta deployment dan observability yang aman.

## Perilaku & cakupan

- Docker Compose development menyediakan PostgreSQL/MySQL customer dan PostgreSQL aplikasi terpisah.
- Web MVP memakai Docker untuk frontend/API/PostgreSQL; Redis optional dan Kubernetes bukan kebutuhan MVP.
- Mulai API + query engine dalam satu service; siapkan boundary agar worker dapat dipisah tanpa perubahan besar protocol frontend.
- Gunakan structured logging Rust melalui tracing; arsitektur siap OpenTelemetry. SQL logging configurable dan tidak membocorkan password/token/private key.
- Target distribusi desktop: Windows MSI/EXE, macOS DMG, Linux AppImage/DEB; signing/auto-update dapat menyusul core stabil.

## Acceptance criteria

- README mendokumentasikan command yang benar-benar tersedia dan setup database disposable untuk integration tests.
- Deployment web, health, konfigurasi, serta rollback diverifikasi di environment terkait sebelum diklaim selesai.
- Angka target performance PRD dibuktikan benchmark dengan kondisi pengukuran.

## Batasan & keputusan lanjutan

Command Rust/frontend/build/tests dan Compose tersedia di README. Paket desktop Linux lokal tersedia; signing/update dan web deployment/observability belum tervalidasi.

## Dokumen terkait

- [Plan utama](../plans/phase-3-web-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; [Web MVP — Approved](../design-docs/2026-10-10-web-mvp.md) menetapkan Phase 3 yang disetujui.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.


## Implementasi web Phase 3 — Berjalan

Satu replica Axum/query engine tanpa Redis/worker; Docker frontend/reverse proxy/API/PostgreSQL dan satu origin HTTPS. Operator menyediakan provider callbacks/Owner awal, versioned secret-file keys, egress policy dan backups. Migration dijalankan terpisah; schema guard/restore/rollback tidak otomatis downgrade. Restart tidak memulihkan live queries atau replay SQL.

Audit tanpa SQL/secret/rows memiliki default retensi 90 hari; rate limits, redacted structured logs, health/readiness/metrics, dan bounded graceful shutdown wajib divalidasi. Nilai/configuration final dan enforced customer-path isolation ketika API membutuhkan control-plane access masih review gate. [Plan Phase 3](../plans/phase-3-web-mvp.md) memisahkan dokumentasi, pipeline, deployment, dan recovery proof.

2026-10-10: kode Phase 3 tersedia; [laporan validasi](../plans/phase-3-web-mvp-validation.md) memisahkan unit/static dari gate API/browser/DNS/TLS/Docker. [Runbook](../plans/phase-3-web-mvp-operations.md) menjelaskan konfigurasi operator. Patch TCP Q15 telah disetujui; hostname TLS dipertahankan dan IP tervalidasi dipin, tetapi runtime TLS/egress belum dibuktikan.
