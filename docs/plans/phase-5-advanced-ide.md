# Phase 5 — Advanced Database IDE

**Status:** Berjalan — desain; implementasi belum dimulai.

**Dependensi:** desain berjalan sekarang; acceptance/release tetap memerlukan gate Phase 2–4 yang relevan.

**Desain terpilih:** [ER Diagram Read-only — Draft](../design-docs/2026-10-12-phase-5-er-diagram.md).

**Sumber:** PRD §§4, 44, 52.

**Fitur:** [advanced IDE](../features/advanced-ide.md), [operasional](../features/deployment-observability.md).

## Milestone & checklist roadmap

- [x] Prioritaskan kebutuhan: ER diagram read-only dipilih melalui Q1–Q12 dan konfirmasi rangkuman 2026-10-12 pada desain; kandidat lain tetap future scope. Bukti keputusan, bukan runtime.
- [x] Recon graph/source, grilling dan tulis/register Draft ER diagram: 2026-10-12; bukti serta batas pemeriksaan dokumentasi dicatat di bawah. Belum review/approval implementasi.
- [ ] Buat design doc terpisah per perubahan besar dan review sebelum implementasi.
- [ ] Evaluasi schema diff, schema migration, ER diagram, dan DDL generator.
- [ ] Evaluasi refactoring, advanced autocomplete, go to definition, dan dependency graph.
- [ ] Evaluasi database agent dan SSH tunnel dengan boundary akses serta pengelolaan secret yang jelas.
- [ ] Evaluasi enterprise auth dan self-hosted deployment.
- [ ] Evaluasi packaging Windows MSI/EXE, macOS DMG, Linux AppImage/DEB; code signing dan auto-update setelah core stabil.
- [ ] Tetapkan acceptance criteria, compatibility, migrasi/reversibility, security, performance, dan operational rollback untuk setiap fitur yang dipilih.

## Acceptance criteria

Setiap fitur terpilih memiliki desain, scope, acceptance criteria, serta bukti validasi sendiri sebelum dinyatakan selesai. Core tetap independen dari platform dan adapters tetap memiliki perilaku engine-specific. Fitur yang tidak dipilih tetap future scope dan tidak menghambat status MVP.

### Milestone awal — ER diagram read-only

Scope/budget dan keputusan Q1–Q12 mengikuti desain terpilih, bukan sumber keputusan paralel.

- [ ] Review manusia Draft serta approval desain sebelum implementasi.
- [ ] Review DTO/IPC/HTTP/OpenAPI/capabilities, partial/error dan mekanisme disable additive.
- [ ] Spike FK terstruktur/berurutan, incoming/outgoing bounded dan privilege visibility kedua engine.
- [ ] Shared Rust/adapters/host metadata graph, resource accounting, deadline/cancel dan lifecycle scope.
- [ ] Shared UI/layout worker: pemilihan/perluasan, drag/auto-layout/zoom/pan/search/inspector, refresh dan accessibility.
- [ ] Unit/controlled tests serta regresi query/metadata/session/transaksi; bukti terpisah dari runtime.
- [ ] Integrasi PostgreSQL/MySQL: composite/self/cyclic/cross-schema, identifier khusus, restricted access, concurrent DDL dan partial/failure/cancel.
- [ ] Browser/native kedua engine: interaksi graph, stale results, scope/revoke/logout dan layout sementara tanpa persistence.
- [ ] Benchmark katalog 10.000 objek/subgraph maksimum budget, target render/layout, resource/fairness dan responsiveness.
- [ ] Review acceptance/release dan gate Phase 2–4 relevan; dokumentasikan batas platform/pipeline.

## Keputusan sebelum implementasi

ER diagram read-only desktop/web PostgreSQL/MySQL dipilih; detail kontrak/spike dan reviewer tercatat pada Pertanyaan Terbuka desain. Kandidat lainnya belum dipilih. Tidak ada jadwal, estimasi, atau janji dukungan engine tambahan; review/approval manusia diperlukan sebelum implementasi.

## Validasi & progres

2026-10-12: recon graph/source, Q1–Q12 dan konfirmasi rangkuman/penulisan menghasilkan Draft terdaftar. Struktur template, tautan lokal dan `git diff --check` diperiksa. Bukti dokumentasi saja; belum ada implementasi, integrasi database, browser/native atau benchmark Phase 5. Gate release Phase 2–4 tetap terbuka sesuai plan masing-masing.
