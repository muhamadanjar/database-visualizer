# Dokumentasi Database Studio

Sumber kebutuhan produk: [PRD](../PRD.md). Foundation Rust telah diimplementasikan dan divalidasi lokal. Phase 2–5 masih rencana; status dan bukti implementasi tercatat pada plan/fitur terkait.

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

Plan menggunakan `Belum dimulai`, `Berjalan`, `Terblokir`, atau `Selesai`. Phase 1 berstatus `Selesai`; fase berikutnya `Belum dimulai`. Checklist kosong tidak berarti fitur dihapus; fitur tersebut belum dibuktikan selesai.

Design doc mengikuti `Draft → In Review → Approved → Implemented`; `Rejected` dan `Deprecated` adalah status terminal. Status berubah menjadi `In Review` setelah review manusia dilakukan, `Approved` setelah persetujuan desain dicatat, dan `Implemented` setelah bukti implementasi serta validasi tersedia.

## Batas keputusan awal

Disepakati: dokumentasi lima fase; empat crate Foundation; koneksi tetap per session; satu query aktif per session; streaming dengan buffer terbatas; cancellation nyata. Detail tipe serta default resource ditetapkan melalui grilling dan diimplementasikan; default tersebut belum merupakan hasil benchmark. Lihat [desain Foundation](design-docs/2026-10-09-foundation.md).

Phase 1 harus lulus sebelum desktop dimulai. Fase berikutnya direncanakan secara berurutan; jadwal, penanggung jawab, dan desain teknisnya belum ditetapkan. Phase 5 tetap future scope, bukan syarat MVP.
