# Phase 3 — Web MVP

**Status:** Belum dimulai  
**Dependensi:** core dan komponen UI bersama tervalidasi pada [Phase 2](phase-2-desktop-mvp.md).  
**Sumber:** PRD §§5–6, 12, 22–29, 37–43, 45, 50, 53.  
**Fitur:** [web & workspace](../features/web-auth-workspaces.md), [koneksi](../features/connection-security.md), [query/session](../features/query-sessions-streaming.md), [operasional](../features/deployment-observability.md).

## Milestone & checklist

- [ ] Bahas dan review desain API HTTP/WebSocket, auth/session, role-permission matrix, tenant isolation, schema aplikasi, dan secret store.
- [ ] Implementasikan Axum/Tokio backend yang memakai core; mulai API + query engine dalam satu service.
- [ ] Reuse UI bersama melalui HTTP/WebSocket transport; browser tidak mengakses driver database.
- [ ] Implementasikan login GitHub/Google sesuai mekanisme provider, server-side session, dan cookie Secure/HttpOnly/SameSite.
- [ ] Implementasikan organization/workspace/membership serta Owner/Admin/Developer/Viewer dengan izin per resource.
- [ ] Implementasikan schema users, memberships, workspaces, connections, saved queries, query_history, sessions, audit_events beserta migrasi.
- [ ] Enforce organization/workspace/connection ownership untuk HTTP, WebSocket, dan query execution; uji penolakan akses lintas tenant.
- [ ] Implementasikan backend-managed encrypted credentials dengan secret-store abstraction; jangan kirim kembali password ke browser setelah tersimpan.
- [ ] Implementasikan connection CRUD/test, metadata, query creation/cancellation melalui HTTP; stream lifecycle/results melalui WebSocket.
- [ ] Implementasikan workspace-scoped history/saved queries dan opsi menonaktifkan history per koneksi.
- [ ] Terapkan rate limit sensitif, TLS produksi, redaksi secret, dan pemisahan customer database dari control plane.
- [ ] Sediakan Docker deployment frontend/API/PostgreSQL, health checks, structured logging, dan panduan konfigurasi/rollback.
- [ ] Uji login → workspace → save connection → browse → execute → stream → cancel → history pada kedua engine.

## Acceptance criteria

Alur web PRD §53 lulus; tenant isolation dan permission diuji di backend; kredensial terenkripsi dan tidak terpapar browser/log. Hasil serta cancellation memakai core yang sama dengan desktop. API dan schema terdokumentasi sebelum integrasi UI.

## Keputusan sebelum implementasi

Detail provider auth, role-permission matrix, secret store development/production, schema/migration rollback, origin/session/CSRF policy, WebSocket authentication/reconnect, resource quota, serta target deployment perlu design doc sebelum pekerjaan terkait. Worker terpisah dan Redis belum menjadi kebutuhan MVP.

## Validasi & progres

Belum ada implementasi atau validasi. Catat API/database/security/browser/E2E/deployment sebagai bukti terpisah; sertakan pengujian migrasi dan rollback di environment disposable.
