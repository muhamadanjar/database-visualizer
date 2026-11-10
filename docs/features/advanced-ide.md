# Advanced Database IDE

**Status implementasi:** Belum dimulai; desain ER diagram berstatus Draft

**Fase:** Phase 5

**Sumber:** [PRD](../../PRD.md) §§4, 52.

## Tujuan

Roadmap fitur lanjutan yang dipilih berdasarkan kebutuhan setelah fondasi serta MVP stabil.

## Perilaku & cakupan

- Terpilih untuk milestone awal: ER diagram read-only desktop/web PostgreSQL/MySQL; tabel pilihan/perluasan FK, metadata terstruktur, layout sementara dan resource bounded mengikuti [desain Draft](../design-docs/2026-10-12-phase-5-er-diagram.md).
- Kandidat schema tools berikutnya: schema diff, schema migration, DDL generator.
- Kandidat language tools: refactoring, advanced autocomplete, go to definition, dependency graph.
- Kandidat platform/access: database agent, SSH tunnel, enterprise auth, self-hosted deployment.
- Setiap fitur terpilih membutuhkan desain dan checklist rinci sendiri; daftar kandidat bukan janji implementasi.

## Acceptance criteria

- Scope dan acceptance criteria disepakati sebelum pekerjaan dimulai; risiko compatibility, migrasi, performance, security, operations dievaluasi.
- Seluruh platform tetap reuse core tanpa dependency UI/network di core.
- ER diagram membutuhkan bukti metadata/privilege kedua engine, browser/native, failure paths dan benchmark sesuai desain; belum ada bukti runtime Phase 5.

## Batasan & keputusan lanjutan

Tidak termasuk syarat MVP; engine tambahan, enterprise SSO, dan schema migration lanjutan tidak diimplementasikan pada Foundation.

## Dokumen terkait

- [Plan utama](../plans/phase-5-advanced-ide.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; [ER diagram — Draft](../design-docs/2026-10-12-phase-5-er-diagram.md) mencatat keputusan Q1–Q12. Review/approval implementasi belum dilakukan.
