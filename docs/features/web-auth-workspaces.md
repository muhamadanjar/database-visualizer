# Web Authentication, Workspace & Tenant Isolation

**Status implementasi:** Belum dimulai  
**Fase:** Phase 3  
**Sumber:** [PRD](../../PRD.md) §§22–29, 39, 43, 50, 53.

## Tujuan

Aplikasi web berautentikasi untuk mengelola database customer dalam organisasi dan workspace terisolasi.

## Perilaku & cakupan

- Browser → HTTPS/WebSocket → Axum backend → shared core → customer database; browser tidak mengakses database langsung.
- GitHub/Google menjadi target provider awal dengan OAuth 2/OIDC sesuai kemampuan provider; session server-side dan cookie Secure/HttpOnly/SameSite.
- Hierarchy: User, Organization, Workspace, Connection; peran Owner/Admin/Developer/Viewer dengan izin resource-based.
- Izin minimum: workspace:manage, connection:create/update/delete/read, query:execute, metadata:read, secret:manage.
- Semua resource web scoped organization/workspace; lookup berdasarkan ID saja tidak cukup. Authorization HTTP dan WebSocket berada di backend.
- Application PostgreSQL menyimpan users, organizations/memberships, workspaces/memberships, connections, saved_queries, query_history, sessions, audit_events.
- History memuat identitas query/user/connection, query text, status, durasi, row count, dan timestamps; dapat dinonaktifkan per koneksi. Saved queries scoped workspace.

## Acceptance criteria

- Alur login → workspace → secure connection → browse → execute → stream → cancel → history lulus.
- Pengujian membuktikan resource dan query lintas tenant ditolak serta secret tersimpan tidak dikembalikan ke browser.
- Main auth token tidak disimpan di localStorage; UI permission checks hanya membantu UX.

## Batasan & keputusan lanjutan

Role-permission matrix, provider detail, schema/migrasi, secret store, dan WebSocket/session security perlu design doc tersendiri.

## Dokumen terkait

- [Plan utama](../plans/phase-3-web-mvp.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.
