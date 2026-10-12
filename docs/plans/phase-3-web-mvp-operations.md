# Runbook Web MVP

**Status:** instruksi dan artefak tersedia, deployment/recovery production belum dibuktikan.
**Terkait:** [plan](phase-3-web-mvp.md), [desain](../design-docs/2026-10-10-web-mvp.md), [review pinning](../design-docs/phase-3-driver-pinning-review.md).

## Development disposable

Jalankan `docker compose up -d --wait`. PostgreSQL aplikasi memakai `55433`; customer PostgreSQL/MySQL memakai `55432`/`53306`. Test API membuat schema unik pada database aplikasi; jangan gunakan database pengguna.

Siapkan directory operator di luar repository, file `keys.json` mode `0600`, dan `targets.json`. Buat kunci hanya untuk deployment baru yang belum memiliki ciphertext:

```sh
mkdir -m 700 /tmp/studio-web-dev
node -e 'const fs=require("node:fs"),crypto=require("node:crypto");fs.writeFileSync(process.argv[1],JSON.stringify({active:"dev-v1",keys:{"dev-v1":crypto.randomBytes(32).toString("base64")}}),{flag:"wx",mode:0o600})' /tmp/studio-web-dev/keys.json
```

`targets.json` untuk fixture:

```json
[
  {"host":"127.0.0.1","port":55432,"network":"127.0.0.1/32"},
  {"host":"127.0.0.1","port":53306,"network":"127.0.0.1/32"}
]
```

Export konfigurasi berikut di terminal API. Credential contoh adalah fixture disposable:

```sh
export STUDIO_APPLICATION_DATABASE_URL='postgres://studio_app:studio_test@127.0.0.1:55433/studio_app'
export STUDIO_WEB_ORIGIN='http://127.0.0.1:3000' STUDIO_DEVELOPMENT=1
export STUDIO_BOOTSTRAP_OWNER='test:owner'
export STUDIO_KEY_FILE=/tmp/studio-web-dev/keys.json
export STUDIO_TARGET_POLICY_FILE=/tmp/studio-web-dev/targets.json
export STUDIO_SPOOL_DIRECTORY=/tmp/studio-web-dev/results
cargo run -p studio-api --locked -- migrate
cargo run -p studio-api --features test-provider --locked
```

Di terminal frontend: `VITE_TEST_PROVIDER=1 pnpm web`. Login fixture hanya ada pada backend berfitur `test-provider` **dan** development; Docker production dibangun tanpa fitur tersebut. Profil customer memilih TLS `disable` secara eksplisit untuk fixture tanpa sertifikat. Root transport versi/role tersedia pada resource workspace; OpenAPI di `/api/v1/openapi.json`.

## Production satu origin

Gunakan `compose.web.yaml`. Directory operator menyediakan `api.env`, `keys.json`, `targets.json`, `application-password`, optional secret OAuth, dan `tls/fullchain.pem`/`privkey.pem`. API/key/OAuth files dimiliki UID `10001`, mode `0600`; password PostgreSQL memakai UID user image yang diverifikasi operator, private key Nginx UID `101`. Policy non-secret dapat `0644` agar gateway membacanya. Jangan memasukkan directory ini ke Git/build context.

`api.env` menetapkan origin HTTPS tanpa trailing slash, URL PostgreSQL aplikasi dengan literal IP `172.30.1.2:5432` dan password URL-encoded, `STUDIO_BOOTSTRAP_OWNER=github:<subject>` atau `google:<sub>`, serta pasangan `STUDIO_GITHUB_CLIENT_ID`/`STUDIO_GITHUB_CLIENT_SECRET_FILE` dan/atau Google. Callback persis `${STUDIO_WEB_ORIGIN}/api/v1/auth/github/callback` atau `/google/callback`. Secret file berada di `/operator/`; token provider tidak dipersistenkan. Provider identitas berbeda tidak ditautkan melalui email.

Gateway memiliki namespace jaringan sendiri dan capability `NET_ADMIN`; API berbagi namespace itu dengan seluruh capability dihapus. Rules membatasi CIDR/port operator; backend memeriksa exact hostname dan seluruh IP hasil DNS, memblokir metadata endpoint, serta mengecualikan control-plane dan HTTPS provider yang dipercaya. Policy aplikasi tetap melarang customer connection ke tuple PostgreSQL aplikasi. Jangan menjalankan `egress.sh` di host. Perubahan policy memerlukan penghentian API dan rekreasi API/gateway. Patch Q15 disetujui dan diaktifkan: TCP memakai IP tervalidasi, hostname dipertahankan untuk TLS. Bukti DNS/TLS/egress aktual masih wajib; production mewajibkan `verify_full`.

```sh
export STUDIO_OPERATOR_DIRECTORY=/absolute/operator-directory
docker compose -f compose.web.yaml config --quiet
docker compose -f compose.web.yaml up -d --wait application-postgres egress
docker compose -f compose.web.yaml --profile maintenance run --rm migrate
docker compose -f compose.web.yaml up -d --wait api frontend
```

Satu replica ditegakkan melalui advisory lock PostgreSQL dan lock spool. Migrasi/rotasi menolak instance aktif. Health/readiness tidak memuat data; gauges workspace dan audit membutuhkan Owner/Admin. Audit organisasi Owner mempertahankan metadata workspace yang dihapus. Logs memakai route template/status/durasi, tanpa query string, cookie, SQL, rows, atau credential.

## Backup, rotasi, recovery

Sebelum perubahan: hentikan API, buat `pg_dump -Fc` database aplikasi, verifikasi backup, dan amankan salinan seluruh versi key secara terpisah. Migrasi v1→v2 transaksi; schema lebih baru ditolak tanpa downgrade otomatis. Kegagalan migrasi tidak boleh diatasi dengan mengubah nomor schema.

Untuk rotasi, tambahkan key baru pada file yang tetap memuat key lama, ubah `active`, lalu jalankan `studio-api rotate-key` dengan URL database dan `STUDIO_KEY_FILE`. Command transaksi me-rewrap credential, SQL document, dan CSRF; OAuth state sementara dihapus. HMAC idempotency lama **tidak** dapat dihitung ulang tanpa menyimpan SQL, sehingga key lama harus tersedia sampai tombstone 30 hari dan backup lama tidak diperlukan. Verifikasi pembacaan data sebelum restart/retire key.

Restore database dengan versi aplikasi yang kompatibel dan pulihkan key versions yang dirujuk ciphertext. Missing key gagal tertutup; jangan menghasilkan key pengganti untuk data lama. Restart menandai admission aktif `Interrupted/Unknown`, membersihkan spool sementara, dan tidak memulihkan session/transaksi atau menjalankan ulang SQL. SIGTERM/Ctrl-C memulai cleanup dengan deadline 25 detik; efek SQL yang sudah dikirim tidak dijanjikan rollback.

Operator dapat membuat organisasi tambahan untuk identitas yang sudah login: set `STUDIO_PROVISION_OWNER=github:<subject>`/`google:<sub>`, `STUDIO_PROVISION_NAME`, lalu `studio-api provision-organization`. Endpoint public tidak membuat organisasi. Owner memberi membership lewat ID akun; Admin workspace memberikan akses kepada anggota organisasi yang sudah ada.

## Validasi

- `cargo test -p studio-api -p query-runtime --features studio-api/test-provider --locked`: unit dan kompilasi suite fixture.
- Tambahkan `-- --include-ignored` untuk HTTP/WebSocket/DB nyata.
- `pnpm typecheck:web`, `pnpm build:web`, `pnpm test`: type/build/unit transport; mock bukan browser proof.
- Setelah host fixture berjalan dan `playwright-cli 0.1.19`/Chrome tersedia: `pnpm test:web-browser`.

Workflow [web](../../.github/workflows/web.yml) menyiapkan fixture dan scenario browser. Jangan mengunggah storage state, header, trace, atau CLI reports yang dapat memuat autentikasi. Screenshot fixture saja disimpan di `test-results/web/`. Login GitHub/Google nyata, deployment TLS/egress, restore/rotation, DNS certificate mismatch, stress 100.000 rows, dan pipeline aktual tetap memerlukan bukti terpisah pada [laporan validasi](phase-3-web-mvp-validation.md).
