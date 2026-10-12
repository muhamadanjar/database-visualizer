# Katalog Fitur

Dokumen menjelaskan kebutuhan, perilaku, status implementasi, dan bukti terkait. Foundation tersedia; desktop Phase 2 tervalidasi lokal Linux dengan gate platform tersisa. Kode Web tersedia dengan acceptance terbuka; kode Phase 4 tersedia dengan acceptance terbuka. Sumber produk: [PRD](../../PRD.md).

| Fitur | Fase | Implementasi |
|---|---|---|
| [Shared Database Core & Adapter](database-core-adapters.md) | Phase 1 | Foundation selesai; perluasan core Phase 2 tervalidasi pada kedua engine |
| [Connection Manager & Credential Security](connection-security.md) | Phase 1: koneksi runtime; Phase 2: desktop persistence; Phase 3: web persistence | Runtime dan connection CRUD desktop tervalidasi lokal; OS store native belum tersedia di mesin Linux ini; kode web tersedia, gate runtime terbuka |
| [Database Explorer, Inspector & Table Viewer](database-explorer-inspector.md) | Phase 1: metadata; Phase 2–3: UI desktop/web | Metadata lengkap kedua engine dan inspector desktop tervalidasi lokal Linux; benchmark katalog UI belum lulus |
| [Query Execution, Session, Streaming & Cancellation](query-sessions-streaming.md) | Phase 1: core; Phase 2–3: transport/UI | Foundation dan transport/host/UI desktop tervalidasi lokal; stress ACK native dan web masih terbuka |
| [SQL Editor & Result Grid](sql-editor-result-grid.md) | Phase 2: desktop; Phase 3: reuse web; Phase 4: peningkatan | Monaco, script, transaksi, dan grid 100.000 rows tervalidasi native Linux; acceptance tiga OS masih terbuka |
| [Desktop Workspace & Local Persistence](desktop-workspace.md) | Phase 2 | Desktop Linux tersedia dan native E2E lulus; gate tiga OS/persistence native masih terbuka |
| [Web Authentication, Workspace & Tenant Isolation](web-auth-workspaces.md) | Phase 3 | Kode tersedia; acceptance runtime/deployment terbuka |
| [Productivity Tools](productivity-tools.md) | Phase 4; baseline tersedia pada MVP | Kode tersedia; acceptance browser/native/release terbuka |
| [Advanced Database IDE — Future Scope](advanced-ide.md) | Phase 5 | Belum dimulai |
| [Development Environment, Deployment & Observability](deployment-observability.md) | Phase 1: local/test; Phase 3: web deployment; distribusi desktop setelah core stabil | Environment development tersedia; deployment/observability produksi belum dimulai |

Setiap perubahan perilaku harus memperbarui dokumen fitur terkait, [plan](../plans/README.md), dan [desain](../design-docs/README.md) jika keputusan arsitektur/kontrak berubah. Basic EXPLAIN, saved queries, autocomplete, DDL inspection, dan shortcuts tetap baseline MVP; Phase 4 meningkatkan kemampuan tersebut.
