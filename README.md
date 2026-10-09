# Database Studio — Foundation

Database Studio adalah client PostgreSQL/MySQL desktop dan web yang direncanakan dalam [PRD](PRD.md). Repository saat ini menyediakan shared Rust Foundation, adapters, session/query runtime, metadata, serta tests; Tauri/React/Axum belum diimplementasikan. Dokumentasi fase: [docs](docs/README.md).

## Struktur & dependency

- `crates/db-types`: typed config, capabilities, metadata, values, events, errors.
- `crates/db-core`: adapter contracts, session registry, resource limits, SQL validation, bounded streaming, cancellation/cleanup.
- `crates/db-postgres`, `crates/db-mysql`: driver, introspection, decoding, control cancellation.
- `infra/docker`: initialization untuk database test PostgreSQL non-superuser.
- `compose.yaml`: PostgreSQL customer, MySQL customer, dan PostgreSQL aplikasi terpisah.

Adapters bergantung pada kontrak core; core tidak mengimpor adapters, SQLx, UI, Tauri, Axum, atau network transport. Example/tests menghubungkan implementasi konkret.

## Setup & commands

Butuh Docker Compose dan Rust 1.99.0 (lihat `rust-toolchain.toml`). Dependency versions/graph dikunci dalam manifests dan `Cargo.lock`; database image digests juga dikunci. Versi yang diuji: PostgreSQL 16.13, MySQL 8.4.8, SQLx 0.8.6, sqlparser 0.63.0.

```sh
docker compose up -d --wait
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --locked -- --include-ignored
cargo run -p db-core --locked --example foundation -- postgres
cargo run -p db-core --locked --example foundation -- mysql
```

`cargo test` biasa menjalankan policy/unit tests tanpa database. Integration tests ditandai `ignored` karena harus diarahkan ke fixture disposable; `--include-ignored` menjalankan seluruh suite, termasuk spike driver/parser. Example menampilkan metadata count dan SELECT streamed melalui core. Tidak ada `pnpm dev` atau desktop/web executable saat ini.

Default fixture customer: PostgreSQL `127.0.0.1:55432`, MySQL `127.0.0.1:53306`, user `studio`, database `studio_test`. PostgreSQL aplikasi memakai port `55433` dan user/database `studio_app`. Credential dalam `.env.example` adalah fixture lokal disposable, bukan production secret. Untuk custom port/password, export `STUDIO_PG_PORT`, `STUDIO_MYSQL_PORT`, `STUDIO_APP_PORT`, `STUDIO_TEST_PASSWORD` ke shell; Compose dan example membacanya. Integration tests membaca variable yang sama. `.env` Compose tidak otomatis diekspor ke proses Rust.

PostgreSQL test user adalah non-superuser; MySQL hanya diberi akses ke database fixture. Tests dapat membuat/menghapus tabel, view, dan sequence dengan nama unik. Jangan arahkan ke database pengguna/produksi. `docker compose down` menghentikan environment proyek ini; data disposable hilang ketika container direkreasi karena belum ada persistent volume.

## Runtime contract

Satu editor session memegang koneksi tetap dan maksimal satu query aktif. Session sibuk ditolak; reconnect membuat session baru tanpa replay atau pemulihan transaksi. Tidak ada idle eviction. Gunakan `disconnect`/`shutdown`; host terakhir yang di-drop memberi sinyal cleanup ke query aktif. Host production harus melakukan shutdown terawait sebelum menghentikan runtime Tokio.

Default: editor 4/profil dan 16/proses; metadata serta kontrol cancellation masing-masing 1/profil dan 4/proses. Kuota menghitung koneksi yang sedang dibuka. Metadata memakai koneksi terpisah dan tidak melihat transaksi/temp table editor. Profil diidentifikasi oleh `ConnectionConfig.id`; host harus menjaga ID dan ownership profil.

Batch maksimal 256 rows atau 1 MiB payload; antrean 4 MiB/query plus batas jumlah pesan. Producer menunggu ketika penuh; tertahan 30 detik memicu cancellation. Row terlalu besar gagal tanpa truncation. Byte accounting menghitung payload nilai/column metadata, bukan allocator overhead; pending batch dan current driver row berada di luar antrean. SQLx dapat mengalokasikan message/row besar sebelum ukuran diperiksa: ini bukan batas keras total RSS.

Connect timeout 10 detik pada contoh/config; query timeout opsional. `cancel(query_id)` mengajukan permintaan, bukan bukti query berhenti. Event terminal diterbitkan sekali; completion yang menang race tetap Completed. Summary `results_complete=false` menandakan rows dibuang saat cancellation walaupun driver melaporkan sukses; consumer harus menampilkan hasil parsial. Cancellation/cleanup mempunyai satu deadline 5 detik. Ketidakpastian menjadi `QUERY_OUTCOME_UNKNOWN` dan session invalid. Failed/Cancelled tidak menjamin rollback; rollback tidak dilakukan diam-diam. Status transaksi `Unknown` karena SQLx wrapper depth tidak melacak arbitrary BEGIN SQL secara authoritative.

## SQL, values & security boundaries

SQL asli dikirim tanpa rewrite setelah validasi parser dialect. Foundation hanya menerima satu statement dan menolak CALL, PREPARE/EXECUTE, COPY serta MySQL executable comments; blok DO PostgreSQL belum didukung parser. Syntax database valid yang tidak dipahami parser mendapat `UNSUPPORTED_STATEMENT`. Script/multiple results harus dirancang sebelum full editor Phase 2.

Nilai mempertahankan NULL, integer, float (string round-trip), exact decimal, text/binary, temporal, JSON, atau opaque engine type/format/payload. Unsupported/invalid decode tidak dipotong atau diubah menjadi nilai tebakan. Metadata tetap menyimpan nama tipe engine. MySQL BOOLEAN adalah alias TINYINT(1); nilai selain 0/1 tetap integer. Temporal offsets mengikuti tipe asli; jangan menganggap timestamp tanpa offset sebagai UTC.

Connection config tidak serializable dan Debug redacted; driver statement logging dimatikan. Error publik hanya pesan aman dan database code, tanpa SQL/driver message/credential. Host wajib mengisi `ProtectedTarget` dengan seluruh endpoint/alias control plane yang harus ditolak. Guard ini membandingkan endpoint konfigurasi, bukan DNS-aware tenant authorization; host web nanti harus memvalidasi ownership dan kebijakan target di backend. Core memakai koneksi customer terpisah, bukan application pool.

## Evidence & limits

Lihat [Phase 1 validation](docs/plans/phase-1-foundation.md). Driver spike membuktikan session/temp table, cancellation/recovery dengan akun biasa, dan batas parser. Integration tests meliputi metadata, typed values, streaming, backpressure, oversized row, blocked lock, reconnect, consumer drop. Failure-path policy tests memakai adapter terkontrol dan tidak menggantikan bukti database.

Belum membuktikan UI/native/browser/E2E, TLS deployment produksi, semua tipe engine, permission matrix web, pipeline, hard RSS ceiling, atau target performance desktop. Core Foundation tidak berarti MVP selesai.
