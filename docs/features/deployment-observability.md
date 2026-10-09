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

Command Rust/build/tests/example dan Compose tersedia di README. Frontend dev, hosting, monitoring detail, packaging, signing, dan update policy belum diimplementasikan.

## Dokumen terkait

- [Plan utama](../plans/phase-3-web-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.
