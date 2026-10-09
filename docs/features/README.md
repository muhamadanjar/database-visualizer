# Katalog Fitur

Dokumen menjelaskan kebutuhan, perilaku, status implementasi, dan bukti terkait. Foundation tersedia; fitur platform lanjutan masih direncanakan. Sumber produk: [PRD](../../PRD.md).

| Fitur | Fase | Implementasi |
|---|---|---|
| [Shared Database Core & Adapter](database-core-adapters.md) | Phase 1 | Selesai untuk Foundation; platform lanjutan belum dimulai |
| [Connection Manager & Credential Security](connection-security.md) | Phase 1: koneksi runtime; Phase 2: desktop persistence; Phase 3: web persistence | Phase 1 koneksi runtime tervalidasi; penyimpanan credential desktop/web belum dimulai |
| [Database Explorer, Inspector & Table Viewer](database-explorer-inspector.md) | Phase 1: metadata; Phase 2–3: UI desktop/web | Metadata Foundation tervalidasi; UI explorer/inspector dan metadata lengkap MVP belum dimulai |
| [Query Execution, Session, Streaming & Cancellation](query-sessions-streaming.md) | Phase 1: core; Phase 2–3: transport/UI | Core Phase 1 tervalidasi; transport/UI Phase 2–3 belum dimulai |
| [SQL Editor & Result Grid](sql-editor-result-grid.md) | Phase 2: desktop; Phase 3: reuse web; Phase 4: peningkatan | Belum dimulai |
| [Desktop Workspace & Local Persistence](desktop-workspace.md) | Phase 2 | Belum dimulai |
| [Web Authentication, Workspace & Tenant Isolation](web-auth-workspaces.md) | Phase 3 | Belum dimulai |
| [Productivity Tools](productivity-tools.md) | Phase 4; baseline tersedia pada MVP | Belum dimulai |
| [Advanced Database IDE — Future Scope](advanced-ide.md) | Phase 5 | Belum dimulai |
| [Development Environment, Deployment & Observability](deployment-observability.md) | Phase 1: local/test; Phase 3: web deployment; distribusi desktop setelah core stabil | Environment development tersedia; deployment/observability produksi belum dimulai |

Setiap perubahan perilaku harus memperbarui dokumen fitur terkait, [plan](../plans/README.md), dan [desain](../design-docs/README.md) jika keputusan arsitektur/kontrak berubah. Basic EXPLAIN, saved queries, autocomplete, DDL inspection, dan shortcuts tetap baseline MVP; Phase 4 meningkatkan kemampuan tersebut.
