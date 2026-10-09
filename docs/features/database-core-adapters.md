# Shared Database Core & Adapter

**Status implementasi:** Selesai untuk Foundation; platform lanjutan belum dimulai  
**Fase:** Phase 1  
**Sumber:** [PRD](../../PRD.md) §§6–10, 37, 48, 54–57.

## Tujuan

Fondasi Rust independen yang dipakai desktop, web, dan platform berikutnya untuk operasi PostgreSQL/MySQL.

## Perilaku & cakupan

- `db-types` menyimpan tipe bersama; `db-core` menyimpan kontrak serta koordinasi session/query; adapter mengimplementasikan perilaku engine.
- Capabilities menyatakan fitur yang benar-benar tersedia; MySQL tidak dipaksa mempunyai PostgreSQL sequences atau schemas.
- Metadata/query lifecycle berupa modul dalam empat crate Foundation; driver tidak menjadi detail UI.
- Gunakan stable Rust, typed errors, dan explicit types; hindari `unwrap()` pada production paths.

## Acceptance criteria

- Connect, metadata inspection, execute, streaming, cancel, dan disconnect lulus integrasi kedua engine sesuai PRD §56.
- Tidak ada dependency core terhadap Tauri, Axum, React, HTTP, atau WebSocket.

## Batasan & keputusan lanjutan

Tidak mencakup engine tambahan atau semua crate target PRD sekaligus.

## Dokumen terkait

- [Plan utama](../plans/phase-1-foundation.md); lihat [indeks fase](../plans/README.md) untuk platform/fase lanjutan.
- [Desain Foundation](../design-docs/2026-10-09-foundation.md) menetapkan batas core; desain fase berikutnya belum tersedia.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Status ini tidak menyatakan UI/transport/platform MVP sudah tersedia.
