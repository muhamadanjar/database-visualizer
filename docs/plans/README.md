# Rencana Fase

Phase 1 telah divalidasi lokal; Phase 2–5 belum dimulai. Lihat bukti dan keterbatasan pada masing-masing plan.

| Fase | Plan | Dependensi | Status |
|---|---|---|---|
| 1 — Foundation | [Checklist rinci](phase-1-foundation.md) | Review desain Foundation dan penyelesaian keputusan penghambat | Selesai |
| 2 — Desktop MVP | [Milestone desktop](phase-2-desktop-mvp.md) | Acceptance criteria Phase 1 lulus | Belum dimulai |
| 3 — Web MVP | [Milestone web](phase-3-web-mvp.md) | Core tervalidasi dan UI bersama dari Phase 2 | Belum dimulai |
| 4 — Productivity | [Milestone produktivitas](phase-4-productivity.md) | Desktop/Web MVP tervalidasi | Belum dimulai |
| 5 — Advanced Database IDE | [Roadmap lanjutan](phase-5-advanced-ide.md) | Evaluasi kebutuhan setelah Phase 4 | Belum dimulai; future scope |

## Aturan checklist

- Gunakan `[ ]` untuk pekerjaan belum selesai dan `[x]` untuk hasil yang telah divalidasi.
- Setiap penyelesaian mencantumkan tanggal, artefak/perubahan, perintah atau langkah validasi, hasil, dan keterbatasan bukti.
- Pisahkan static check, unit test, integrasi database, browser, E2E, benchmark, dan pipeline. Keberhasilan satu kategori tidak membuktikan kategori lain.
- Catat penghambat, keputusan desain, serta dependensi; jangan mencentang item hanya karena kode ditulis.
- Link ke [fitur](../features/README.md) dan [design doc](../design-docs/README.md) sebagai konteks, bukan salin keputusan ke sumber paralel.
- Perubahan requirement harus menyebut bagian PRD dan alasan penyimpangan.
