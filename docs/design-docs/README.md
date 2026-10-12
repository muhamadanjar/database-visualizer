# Design Docs

Design doc diperlukan untuk perubahan arsitektur, komponen/service baru, schema signifikan, API publik baru/breaking, perubahan lintas service, atau operasi/data berisiko. Perubahan kecil tanpa perubahan kontrak/perilaku tidak memerlukan design doc.

## Menulis & mereview

- Gunakan [TEMPLATE.md](TEMPLATE.md) dan pertahankan urutan serta semua section; isi yang tidak relevan dengan `N/A` beserta alasan.
- Nama file: `YYYY-MM-DD-judul-singkat.md`, lowercase dengan hyphen. Gunakan bahasa Indonesia.
- Mulai dari fakta repository/PRD, bahas keputusan dan alternatif, lalu konfirmasi pemahaman sebelum menulis.
- Catat author/reviewer/ticket nyata bila diketahui; jangan mengarang identitas. Data belum diketahui memakai `[perlu konfirmasi: ...]`.
- Cantumkan minimal dua alternatif dengan trade-off serta lima sumbu risiko: compatibility, migrasi/reversibility, performance, security, operations/rollback.
- Pertanyaan terbuka wajib, termasuk asumsi dan keputusan yang menghambat implementasi. Tautkan fitur dan plan, hindari sumber keputusan ganda.

## Status

`Draft → In Review → Approved → Implemented`, dengan `Rejected` atau `Deprecated` sebagai terminal. `In Review` hanya setelah review manusia dilakukan; catat reviewer dan hasil review. Persetujuan arah desain dalam diskusi awal tidak otomatis mengubah status dokumen menjadi `Approved`. `Implemented` memerlukan bukti pekerjaan serta validasi dari plan terkait.

## Indeks

| Dokumen | Dibuat | Status | Fitur / Plan |
|---|---|---|---|
| [Foundation](2026-10-09-foundation.md) | 2026-10-09 | Implemented | [Core](../features/database-core-adapters.md), [query/session](../features/query-sessions-streaming.md), [Phase 1](../plans/phase-1-foundation.md) |
| [Desktop MVP](2026-10-09-desktop-mvp.md) | 2026-10-09 | Approved | [Desktop](../features/desktop-workspace.md), [editor/grid](../features/sql-editor-result-grid.md), [Phase 2](../plans/phase-2-desktop-mvp.md) |
| [Web MVP](2026-10-10-web-mvp.md) | 2026-10-10 | Approved | [Web/workspace](../features/web-auth-workspaces.md), [Phase 3](../plans/phase-3-web-mvp.md) |
| [Productivity](2026-10-10-productivity.md) | 2026-10-10 | Approved | [Productivity](../features/productivity-tools.md), [Phase 4](../plans/phase-4-productivity.md) |

Desain Desktop/Web dan patch driver disetujui pengguna. Gate acceptance tetap dicatat pada plan; [validasi Web](../plans/phase-3-web-mvp-validation.md) dan [runbook](../plans/phase-3-web-mvp-operations.md) tersedia.

Review transport Phase 3: [pin tujuan TCP](phase-3-driver-pinning-review.md), Approved 2026-10-10; runtime TLS/egress tetap gate.

Artefak Phase 4: [kontrak implementasi/API/schema](phase-4-productivity-contracts.md); bukti dan gate terpisah pada [validasi](../plans/phase-4-productivity-validation.md).
