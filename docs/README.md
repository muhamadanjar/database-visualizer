# Dokumentasi Database Studio

Sumber kebutuhan produk: [PRD](../PRD.md). Foundation Rust telah diimplementasikan dan divalidasi lokal. Phase 2 berjalan dengan kode desktop dan validasi lokal Linux; Phase 3–4 berjalan; Phase 5 masih rencana; status dan bukti implementasi tercatat pada plan/fitur terkait.

| Folder | Isi | Indeks |
|---|---|---|
| `features/` | Apa fitur tersebut, perilaku pengguna, batasan, dan acceptance criteria | [Fitur](features/README.md) |
| `plans/` | Urutan kerja, dependensi, checklist, dan bukti validasi | [Rencana fase](plans/README.md) |
| `design-docs/` | Keputusan arsitektur, alternatif, risiko, dan pertanyaan terbuka | [Design docs](design-docs/README.md) |

## Alur kerja

1. Baca PRD dan dokumen fitur terkait.
2. Untuk perubahan arsitektur, schema, API publik, lintas komponen, atau operasi berisiko, bahas keputusan dan tulis design doc memakai template.
3. Selesaikan keputusan yang menjadi prasyarat pekerjaan; lakukan review manusia sebelum implementasi desain terkait.
4. Kerjakan checklist plan fase aktif. Tandai `[x]` hanya ketika hasil dan validasinya tersedia; tulis bukti di bagian Validasi & progres.
5. Perbarui dokumen fitur, plan, dan desain dalam perubahan yang sama ketika perilaku atau kontrak berubah.

Plan menggunakan `Belum dimulai`, `Berjalan`, `Terblokir`, atau `Selesai`. Phase 1 berstatus `Selesai`, Phase 2–4 `Berjalan`, dan Phase 5 `Belum dimulai`. Checklist kosong tidak berarti fitur dihapus; fitur tersebut belum dibuktikan selesai.

Design doc mengikuti `Draft → In Review → Approved → Implemented`; `Rejected` dan `Deprecated` adalah status terminal. Status berubah menjadi `In Review` setelah review manusia dilakukan, `Approved` setelah persetujuan desain dicatat, dan `Implemented` setelah bukti implementasi serta validasi tersedia.

## Batas keputusan awal

Disepakati: dokumentasi lima fase; empat crate Foundation; koneksi tetap per session; satu query aktif per session; streaming dengan buffer terbatas; cancellation nyata. Detail tipe serta default resource ditetapkan melalui grilling dan diimplementasikan; default tersebut belum merupakan hasil benchmark. Lihat [desain Foundation](design-docs/2026-10-09-foundation.md).

Phase 1 harus lulus sebelum desktop dimulai. [Desain Desktop MVP](design-docs/2026-10-09-desktop-mvp.md) berstatus Approved setelah konfirmasi Q1–Q18, perintah implementasi, dan persetujuan Q15. Kode Phase 2 tersedia; [validasi lokal](plans/phase-2-desktop-mvp-validation.md) belum menutup gate seluruh platform. Phase 5 tetap future scope, bukan syarat MVP.

Desain Web MVP dan patch transport Q15 disetujui pengguna pada 2026-10-10. Kode tersedia; [validasi Phase 3](plans/phase-3-web-mvp-validation.md) dan [runbook](plans/phase-3-web-mvp-operations.md) memisahkan bukti lokal dari gate eksternal.

[Desain Productivity](design-docs/2026-10-10-productivity.md) berstatus Approved setelah Q1–Q21 dan instruksi implementasi 2026-10-11. [Checklist Phase 4](plans/phase-4-productivity.md) mencakup desktop/web, productivity lalu editing, review kontrak/migration dan acceptance; kode tersedia; [validasi Phase 4](plans/phase-4-productivity-validation.md) mencatat bukti serta gate acceptance terbuka. Gate release Phase 2–3 tetap berlaku.
