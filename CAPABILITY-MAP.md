# Peta Kapabilitas: Pengeditan dan Pengelolaan Database

| Modul                      | Tanggung jawab                                                                                                                         | Dependensi                                                           |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `grid-editing-ux`          | Membuat staged editing tabel yang sudah ada mudah ditemukan dan digunakan dari data grid.                                              | CRUD Phase 4 yang sudah ada, table viewer                            |
| `postgis-map-viewer`       | Visualisasi read-only geometri PostGIS point, line, dan polygon.                                                                       | Table viewer; independen dari UX editing grid                        |
| `schema-change-management` | Mengelola DDL struktur tabel: create/rename/drop table; add/alter/rename/drop column; index; primary/unique/foreign/check constraints. | Metadata/DDL PostgreSQL dan MySQL, izin database, Inspector struktur |

Urutan terbaru: `grid-editing-ux` → `schema-change-management` → `postgis-map-viewer`. Pengguna meminta implementasi inline editing dan DDL terlebih dahulu; map viewer tetap direncanakan setelahnya.

Cakupan yang disetujui: grid editing inline tetapi tetap staged melalui Preview/Apply yang sudah ada. Map PostGIS read-only, mencakup point/line/polygon; tile basemap remote hanya setelah opt-in. Schema management mencakup DDL struktur tabel umum dengan SQL preview dan Apply terkonfirmasi. Operasi DROP atau perubahan yang berpotensi kehilangan data meminta pengguna mengetik nama objek. SQL DDL bebas, perubahan database/schema/user/privilege, routine/trigger/view, dan fitur vendor khusus bukan bagian awal.
