# Validasi Phase 3 — Web MVP

**Tanggal:** 2026-10-10. **Status:** Berjalan; kode tersedia, acceptance eksternal belum lengkap.
**Persetujuan:** pengguna memerintahkan implementasi setelah Q1–Q20 dan menyetujui patch transport Q15 secara terpisah. [Desain](../design-docs/2026-10-10-web-mvp.md), [review patch](../design-docs/phase-3-driver-pinning-review.md), [checklist](phase-3-web-mvp.md), [runbook](phase-3-web-mvp-operations.md).

## Artefak implementasi

- `services/api`: Axum/resource API `/api/v1`, OpenAPI 3.1 (39 paths), DDL v1, scoped repositories, GitHub/Google OAuth, cookie/CSRF/tab ownership, roles/memberships, SecretStore encryption, audit/limits dan migrations terpisah.
- `crates/query-runtime`: pemilik shared script/profile/DTO/encrypted spool, hierarchical atomic budgets. Desktop menggunakan modul yang sama.
- `apps/web`: browser transport dengan lifecycle/reconnect/page ACK, lost-admission recovery melalui GET tanpa SQL replay, shared workspace/UI, membership administration dan browser file/clipboard.
- SQLx vendor: pin IP internal pada TCP PostgreSQL/MySQL; hostname TLS dipertahankan. Test/connect/metadata/cancel melalui builder adapter yang sama. Seluruh hasil DNS harus sesuai host/port/CIDR; pin bukan input browser.
- `compose.web.yaml` dan `infra/docker/web`: single-replica API, frontend HTTPS, PostgreSQL aplikasi terpisah, gateway egress; workflow `.github/workflows/web.yml` dan runner `tests/web/workspace.browser.js`.

## Bukti lokal

| Kategori | Perintah/artefak | Hasil dan batas |
|---|---|---|
| Rust unit/controlled regression | `cargo test -p db-core -p db-types -p desktop-runtime -p query-runtime -p studio-api --features studio-api/test-provider --locked` | 24 passed; database/native-dependent tests ignored. Mencakup crypto tenant/key rotation, mixed DNS answers/protected targets, limiter dan rollback hierarchical spool budgets. Tidak membuktikan DNS/TLS aktual. |
| Rust static | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lulus termasuk host Tauri. Empat warning upstream SQLx PostgreSQL tetap tercatat; tidak ada workspace Clippy failure. |
| Frontend unit | `pnpm test` | 9 passed: shared UI dan web transport, termasuk tidak replay SQL setelah response hilang, CSRF/access loss dan respons private stale setelah workspace disposal. |
| Frontend static/build | `pnpm typecheck`, `pnpm typecheck:web`, `pnpm build`, `pnpm build:web`, `pnpm format:check` | Lulus desktop/web. Vite memperingatkan ukuran chunk Monaco; belum merupakan bukti startup/RSS/browser. |
| Kontrak/artefak | Referensi OpenAPI lokal; tautan docs; `git diff --check`; `sh -n infra/docker/web/egress.sh`; `node --check scripts/web-browser.mjs`; `docker compose -f compose.web.yaml config --quiet`; reverse patch applicability | Struktur/syntax valid. Compose diperiksa dengan directory operator sementara, tanpa daemon/image/deployment. |
| Code graph | `index_repository` mode fast | 1.671 nodes, 6.517 edges; source baru terindeks. Bukan runtime proof. |

Log eksekusi lokal sementara: `/tmp/database-studio-phase3-{unit,clippy,front-tests,web-check,desktop-check,web-build,desktop-build,format-check}.log`. Tidak dicommit; hasil dan perintah tersimpan di laporan ini.

## Integrasi yang dicoba

Regresi awal setelah ekstraksi shared runtime menjalankan 40 tests Foundation/desktop, termasuk ignored PostgreSQL/MySQL, dan lulus sebelum patch TCP diaktifkan. Ini bukti ekstraksi awal, bukan seluruh perubahan final atau regresi pinning terbaru. Native Linux Phase 2 mempunyai bukti tersendiri pada [laporan desktop](phase-2-desktop-mvp-validation.md); tidak dianggap pengujian native Phase 3 baru.

Suite HTTP/WS pertama menjalankan satu test expiry/schema/instance guard dengan sukses, tetapi test alur engine gagal pada bentuk request metadata. Bentuk tersebut telah diperbaiki. Percobaan berikutnya terhenti pada koneksi fixture dengan `Operation not permitted` (TCP sandbox), sehingga suite akhir **belum lulus**. Dua integration tests tetap ignored secara default dan dikompilasi dengan `test-provider`; mencakup kedua engine, roles/tenant/private documents, session/execution tab ownership, streaming precision, idempotency, cancellation dan logout. Bukti source/kompilasi tidak membuktikan assertions runtime.

## Gate yang tetap terbuka

- Jalankan suite API/HTTP/WS kedua engine dengan `-- --include-ignored` pada fixture disposable; regresi database/native final setelah pinning.
- Jalankan `pnpm test:web-browser` pada API/Vite/fixture nyata; fixture provider bukan bukti GitHub/Google production.
- OAuth kedua provider nyata, HTTPS/CSRF/revocation, DNS berubah sebelum TCP, certificate match/mismatch/CA, IPv4/IPv6 serta enforced egress/control-plane isolation.
- Deployment/image Docker aktual, migration failure, backup/restore/key recovery/rotation, shutdown/crash/restart/idempotency dan slow/duplicate ACK.
- Stress 100.000 rows/10.000 objects, concurrency/fairness, server/browser memory, disk quotas/cleanup dan pipeline aktual.

Phase 3 tidak ditandai Selesai/Implemented sampai gate acceptance relevan memiliki bukti aktual. Tidak ada deployment production dilakukan.
