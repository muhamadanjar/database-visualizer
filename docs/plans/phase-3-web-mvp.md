# Phase 3 — Web MVP

**Status:** Berjalan — kode tersedia; acceptance eksternal belum lulus.
**Dependensi:** core/UI bersama dan regresi desktop lulus; gate platform Phase 2 tetap terpisah.
**Desain:** [Web MVP — Approved](../design-docs/2026-10-10-web-mvp.md), Q1–Q20 dikonfirmasi pengguna 2026-10-10; implementasi dan patch transport Q15 disetujui pengguna.
**Sumber:** PRD §§5–6, 12, 22–29, 37–43, 45, 50, 53.
**Fitur:** [web/workspace](../features/web-auth-workspaces.md), [koneksi](../features/connection-security.md), [query/session](../features/query-sessions-streaming.md), [operasional](../features/deployment-observability.md).

## Milestone & checklist

### Review dan prerequisites

- [x] Grilling Q1–Q20 dan konfirmasi shared understanding/izin penulisan: 2026-10-10, log keputusan pada Draft; hanya bukti kesepakatan desain.
- [x] Tulis/register Draft dan sinkronkan fitur/plan: 2026-10-10; validasi struktur template dan tautan lokal, bukan bukti implementasi.
- [ ] Review release backend/security/deployment; persetujuan implementasi pengguna sudah dicatat di bawah.
- [ ] Spike validated IP/TLS hostname, DNS changes, IPv4/IPv6, certificate match/mismatch, serta test/connect/metadata/cancel kedua engine; review patch driver tersendiri bila dibutuhkan.
- [ ] Buktikan customer-target egress/control-plane isolation sambil mempertahankan akses repositories aplikasi; hostname allowlist saja tidak cukup.
- [ ] Review kontrak OpenAPI/WS/version/errors/idempotency/crash semantics serta DDL/constraints/indexes/migrations.
- [ ] Tetapkan detail callback/account/session/CSRF/revocation, format ciphertext/AAD/rotation/recovery, serta konfigurasi operator yang masih terbuka.

### Runtime, API, dan persistence

- [ ] Ekstrak shared script/spool/paging/resource modules bertahap; pertahankan core independen dan jalankan regresi Foundation/desktop.
- [ ] Inisialisasi Axum/Tokio satu service/replica dengan HTTP resource API dan OpenAPI sebelum integrasi UI.
- [ ] Migrasikan PostgreSQL aplikasi: identities, organizations/workspaces/memberships, connections/revisions, documents/history, auth sessions, execution/idempotency metadata, audit.
- [ ] Terapkan tenant-scoped repositories, relasi foreign key, owner checks; jangan lookup protected resource memakai UUID saja.
- [ ] Implementasikan login GitHub/Google, operator bootstrap Owner, invitation membership, tanpa organization public registration/email auto-link.
- [ ] Terapkan Owner/Admin/Developer/Viewer sesuai matrix; Viewer metadata saja; table Data memerlukan query permission.
- [ ] Terapkan satu origin HTTPS, cookie Secure/HttpOnly/SameSite, CSRF/WS Origin, idle 30 menit/absolut 12 jam; revoke memulai cleanup tanpa grace.
- [ ] Implementasikan SecretStore backend dengan mounted versioned key, encrypted credentials/SQL, rotation/backup recovery tanpa plaintext fallback.
- [ ] Connection CRUD/test/revisi dan policy host/port/network berlaku pada seluruh jalur customer connection; TLS verified default.
- [ ] Session tetap pribadi per user/tab; script preflight/order/cancel/transactions mengikuti core tanpa SQL replay.
- [ ] Creation execution idempotent: same key/request mengembalikan execution sama; konflik ditolak; restart tidak reexecute.
- [ ] WebSocket demand pages: 256 rows/2 MiB, satu outstanding ACK, timeout 30 detik; HTTP terminal, sequence/reconnect protection.
- [ ] Lease reconnect 30 detik; expiry cancel/close tanpa auto-Commit; completed results sementara 10 menit; crash tidak memulihkan live state.
- [ ] Atomic quotas: sessions 4/user/connection, 8/user, 8/workspace, 16/instance; spool 1 GiB/result dan 1 GiB/user, 2 GiB/workspace, 4 GiB/instance.
- [ ] History/editor pribadi default OFF; retention 30 hari/1.000 history per koneksi; saved queries workspace dengan ownership/revision checks.
- [ ] Reuse UI melalui web transport/capabilities; browser file picker/download/clipboard, tanpa native paths/filesystem server/global shutdown.

### Operations dan acceptance

- [ ] Docker frontend/reverse proxy/API/PostgreSQL, satu replica tanpa Redis/worker; production TLS/origin dan enforced network policy.
- [ ] Migration job/backup/schema compatibility/restore/key recovery/rollback serta graceful shutdown dengan bounded cleanup.
- [ ] Structured redacted logs, rate limits sensitif, health/readiness/metrics, audit tanpa SQL/secret/rows; default audit 90 hari.
- [ ] Rust/API/DB tests kedua engine, role/tenant negative tests pada HTTP/WS/page/cancel/documents dan membership revoke.
- [ ] Browser E2E kedua engine: login/workspace/connection/browse/inspect/execute/stream/cancel/transactions/opt-in history.
- [ ] Login GitHub dan Google nyata diuji tersendiri; provider mock bukan bukti OAuth production.
- [ ] Uji duplicate execution/admission crash/restart, reconnect/logout/expiry, ACK slow/duplicate, disk quota/cleanup, encrypted store/rotation, migration failure/restore.
- [ ] Ukur 100.000 rows, katalog 10.000+ object, server/browser memory, fairness dan concurrent consumers; catat kondisi benchmark.
- [ ] Jalankan pipeline dan deployment disposable; konfigurasi/workflow yang baru ditulis tidak dianggap lulus.

## Acceptance criteria

Alur web PRD §53 lulus pada PostgreSQL/MySQL melalui browser. Tenant/resource/owner authorization terbukti pada HTTP dan WebSocket. Viewer tidak dapat melihat data atau menjalankan SQL. Credential/SQL persisten terenkripsi; browser tidak menerima kembali secret tersimpan. Runtime shared tetap lossless/bounded dan cancellation/transaction semantics sama dengan desktop.

Desktop regression, DNS/TLS/egress isolation, provider nyata, privacy/persistence, migration/restore, restart/idempotency, Docker deployment, serta performance/stress memiliki bukti terpisah sebelum Phase 3 Selesai. Gate desktop yang belum lulus tidak ditandai selesai oleh pengujian web.

## Keputusan dan blockers

Pengguna menyetujui implementasi setelah Q1–Q20 dan menyetujui [patch Q15](../design-docs/phase-3-driver-pinning-review.md) secara terpisah pada 2026-10-10. Tidak ada izin deploy production tersirat. Kontrak, DDL, auth/crypto dan operator settings sudah dituangkan dalam kode/OpenAPI/runbook. Akses TCP sandbox ditolak (`Operation not permitted`), sehingga acceptance API/DB/browser serta DNS/TLS belum dapat dituntaskan.

## Validasi & progres

- [x] Persetujuan implementasi dan patch transport: 2026-10-10, jawaban pengguna dicatat pada review Q15; bukan bukti TLS runtime.
- [x] Artefak backend, web/shared runtime, OpenAPI/DDL, Docker, runner browser dan CI tersedia: 2026-10-10; kompilasi/typecheck dan hasil terperinci pada [laporan validasi](phase-3-web-mvp-validation.md). Checkbox acceptance di atas tetap terbuka jika memerlukan integrasi/runtime.

Lihat [runbook lokal/operasional](phase-3-web-mvp-operations.md) untuk migration terpisah, keys, policy, provider, backup/rotation dan shutdown. Phase 3 tetap Berjalan sampai seluruh acceptance yang relevan mempunyai bukti aktual.
