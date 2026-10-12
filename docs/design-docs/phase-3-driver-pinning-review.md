# Review patch pinning tujuan Phase 3

**Status:** Approved — pengguna menyetujui patch dan validasi pada 2026-10-10.
**Keputusan:** [Web MVP Q15](2026-10-10-web-mvp.md). Persetujuan accessor status Phase 2 tidak mencakup perubahan transport ini.

## Patch konkret

[Diff yang dapat direview](phase-3-destination-pinning.patch) menambahkan `tcp_address: Option<IpAddr>` dan setter pada opsi SQLx PostgreSQL/MySQL 0.8.6. TCP menggunakan literal IP yang sudah disahkan operator; `options.host` tetap digunakan SNI/verifikasi identitas TLS. Pin melewati Unix socket. Tanpa pin, perilaku upstream tetap dipertahankan. Patch tidak mengubah executor, parser, handshake autentikasi, atau decoding rows.

## Integrasi setelah persetujuan

Backend menyelesaikan DNS sebelum koneksi, memvalidasi **seluruh** alamat terhadap exact host/port + CIDR operator, menolak control-plane/cloud metadata, lalu mengikat satu IP pada `ConnectionConfig` internal. Pin yang sama berlaku untuk test/connect/metadata, koneksi kontrol cancel, dan reconnect; pembaruan DNS memerlukan validasi baru. Nilai pin bukan input browser dan bukan bagian profil persisten. Koneksi application PostgreSQL menggunakan jalurnya sendiri.

## Validasi yang wajib

- Patch applicability dan kompilasi kedua driver; regresi desktop/core tanpa pin.
- TLS hostname cocok/tidak cocok, CA tidak dipercaya, IPv4/IPv6.
- DNS berubah sebelum TCP: koneksi tetap menuju pin yang disahkan; jawaban campuran allowed/denied ditolak.
- Test/connect/metadata/cancel kedua engine menggunakan tujuan yang sama.
- Egress aktual tetap membatasi jaringan customer/control-plane.

2026-10-10: pengguna menjawab “Ya, setujui patch dan lanjutkan validasi”. Diff diterapkan pada vendor; adapter PostgreSQL/MySQL memakai pin internal termasuk koneksi kontrol cancellation. Backend memvalidasi semua jawaban DNS sebelum memilih IP. Desktop tanpa pin tetap mengikuti upstream. Kompilasi/unit dicatat di [laporan](../plans/phase-3-web-mvp-validation.md); DNS/TLS/egress runtime belum dibuktikan karena akses fixture TCP sandbox ditolak.
