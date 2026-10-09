# Phase 5 — Advanced Database IDE

**Status:** Belum dimulai — future scope  
**Dependensi:** evaluasi kebutuhan setelah [Phase 4](phase-4-productivity.md).  
**Sumber:** PRD §§4, 44, 52.  
**Fitur:** [advanced IDE](../features/advanced-ide.md), [operasional](../features/deployment-observability.md).

## Milestone & checklist roadmap

- [ ] Prioritaskan kebutuhan dan pilih fitur yang layak; daftar ini bukan komitmen implementasi seluruh fitur.
- [ ] Buat design doc terpisah per perubahan besar dan review sebelum implementasi.
- [ ] Evaluasi schema diff, schema migration, ER diagram, dan DDL generator.
- [ ] Evaluasi refactoring, advanced autocomplete, go to definition, dan dependency graph.
- [ ] Evaluasi database agent dan SSH tunnel dengan boundary akses serta pengelolaan secret yang jelas.
- [ ] Evaluasi enterprise auth dan self-hosted deployment.
- [ ] Evaluasi packaging Windows MSI/EXE, macOS DMG, Linux AppImage/DEB; code signing dan auto-update setelah core stabil.
- [ ] Tetapkan acceptance criteria, compatibility, migrasi/reversibility, security, performance, dan operational rollback untuk setiap fitur yang dipilih.

## Acceptance criteria

Setiap fitur terpilih memiliki desain, scope, acceptance criteria, serta bukti validasi sendiri sebelum dinyatakan selesai. Core tetap independen dari platform dan adapters tetap memiliki perilaku engine-specific. Fitur yang tidak dipilih tetap future scope dan tidak menghambat status MVP.

## Keputusan sebelum implementasi

Prioritas, cakupan, target pengguna, deployment, dan kontrak fitur lanjutan belum ditetapkan. Tidak ada jadwal, estimasi, atau janji dukungan engine tambahan. Jangan implementasikan non-goals MVP secara spekulatif.

## Validasi & progres

Belum ada implementasi atau validasi. Checklist evaluasi bukan bukti fitur berjalan; catat keputusan pemilihan/penolakan dan tautkan plan rinci ketika fitur dipilih.
