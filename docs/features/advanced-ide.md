# Advanced Database IDE — Future Scope

**Status implementasi:** Belum dimulai  
**Fase:** Phase 5  
**Sumber:** [PRD](../../PRD.md) §§4, 52.

## Tujuan

Roadmap fitur lanjutan yang dipilih berdasarkan kebutuhan setelah fondasi serta MVP stabil.

## Perilaku & cakupan

- Kandidat schema tools: schema diff, schema migration, ER diagram, DDL generator.
- Kandidat language tools: refactoring, advanced autocomplete, go to definition, dependency graph.
- Kandidat platform/access: database agent, SSH tunnel, enterprise auth, self-hosted deployment.
- Setiap fitur terpilih membutuhkan desain dan checklist rinci sendiri; daftar kandidat bukan janji implementasi.

## Acceptance criteria

- Scope dan acceptance criteria disepakati sebelum pekerjaan dimulai; risiko compatibility, migrasi, performance, security, operations dievaluasi.
- Seluruh platform tetap reuse core tanpa dependency UI/network di core.

## Batasan & keputusan lanjutan

Tidak termasuk syarat MVP; engine tambahan, enterprise SSO, dan schema migration lanjutan tidak diimplementasikan pada Foundation.

## Dokumen terkait

- [Plan utama](../plans/phase-5-advanced-ide.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.
