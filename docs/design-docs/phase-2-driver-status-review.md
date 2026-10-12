# Review patch status protocol SQLx 0.8.6

Proposal pendukung [Desktop MVP](2026-10-09-desktop-mvp.md), gate Q15. **Status: Approved; patch diterapkan.** Pengguna menyetujui patch minimal dan melanjutkan validasi melalui jawaban Q15.

Vendorkan hanya `sqlx-postgres` dan `sqlx-mysql` versi 0.8.6 dari registry, dengan license upstream dan checksum provenance. Tidak mengubah parser, wire protocol, executor, cancellation, atau API transaksi wrapper. Tambahkan dua accessor read-only:

```rust
// PgConnection: baca ReadyForQuery terakhir setelah synchronize selesai.
pub fn protocol_transaction_status(&self) -> u8 {
    match self.inner.transaction_status {
        TransactionStatus::Idle => b'I',
        TransactionStatus::Transaction => b'T',
        TransactionStatus::Error => b'E',
    }
}

// MySqlConnection: baca status server dari respons OK/EOF terakhir.
pub fn protocol_in_transaction(&self) -> bool {
    self.inner.status_flags.intersects(Status::SERVER_STATUS_IN_TRANS)
}
```

Accessor PostgreSQL harus memakai nama variant yang benar pada source pinned; return value wire `I/T/E` menjadi batas patch. Adapter memetakan nilai tersebut ke Idle/Active/Failed; MySQL hanya Idle/Active, tanpa failed-block PostgreSQL. Core mengembalikan Unknown ketika session busy/invalid atau synchronize belum berhasil. Wrapper transaction depth tidak digunakan.

Cargo `[patch.crates-io]` menunjuk dua package lokal. Maintenance: pertahankan diff minimal, license/checksum, dan regression tests; review ulang ketika SQLx di-upgrade. Rollback: lepaskan patch dan accessor adapter, kembali ke Unknown, serta jangan mengklaim gate transaksi lulus.

Validasi wajib: arbitrary BEGIN/COMMIT/ROLLBACK, PostgreSQL error/cancel dalam transaksi, MySQL DDL implicit commit, reconnect/invalid session, dan seluruh Foundation tests. Tidak menambah privilege database.

Alternatif: pindah driver (perubahan executor/type/cancel lebih besar), observasi via catalog (memerlukan privilege dan tidak mencerminkan seluruh state kedua engine), atau tetap Unknown (belum memenuhi MVP).


## Bukti implementasi — 2026-10-10

`vendor/` mempertahankan license/provenance SQLx 0.8.6. Accessor membaca status protocol terakhir setelah adapter menyelesaikan sinkronisasi; executor dan wire protocol tetap upstream. Lihat implementasi pinned untuk nama enum/status aktual.

`crates/db-core/tests/desktop_transactions.rs` lulus pada kedua engine: arbitrary BEGIN/COMMIT/ROLLBACK, PostgreSQL Failed setelah error/cancel, MySQL Active setelah cancel dan Idle setelah implicit commit DDL. Regression Foundation ikut lulus pada [laporan validasi](../plans/phase-2-desktop-mvp-validation.md). Empat warning upstream PostgreSQL dicatat terpisah; tidak diperluas menjadi patch tambahan.
