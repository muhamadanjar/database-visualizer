# Shared Database Core & Adapter

**Status implementasi:** Foundation selesai; perluasan core Phase 2 tervalidasi pada kedua engine
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
- [Foundation](../design-docs/2026-10-09-foundation.md) menetapkan baseline; [Desktop MVP — Approved](../design-docs/2026-10-09-desktop-mvp.md) merencanakan script, metadata lengkap, dan status transaksi authoritative dengan regression gate Foundation.

## Bukti Phase 1

Lihat [validasi Foundation](../plans/phase-1-foundation.md) untuk commands, hasil, dan batas bukti. Bukti Foundation berbeda dari validasi desktop dan tidak menutup acceptance seluruh platform.


## Bukti Phase 2 — 2026-10-10

Lihat [laporan validasi lokal](../plans/phase-2-desktop-mvp-validation.md): 40 Rust tests, 5 frontend tests, dan 2 native Linux E2E pada PostgreSQL/MySQL. Bukti mencakup metadata/inspector, script/transaksi/cancel, grid 100.000 rows, serta clipboard OS. Controlled-store tests membuktikan encrypted persistence; OS keystore nyata tidak tersedia di mesin ini. Windows/macOS, recovery native, dan benchmark lengkap tetap gate; web adalah Phase 3.
