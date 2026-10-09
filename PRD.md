# Product Requirements Document

## 1. Product Name

Working title: **Database Studio**

Nama produk masih sementara dan dapat diganti kemudian.

---

# 2. Product Summary

Database Studio adalah aplikasi database management dan SQL IDE modern yang tersedia dalam dua platform:

- Desktop
- Web

Produk ditujukan sebagai alternatif yang lebih ringan dan modern terhadap tools seperti DBeaver dan DataGrip.

Versi awal harus mendukung:

- PostgreSQL
- MySQL

Arsitektur harus memungkinkan penambahan database engine lain di masa depan tanpa menulis ulang UI, query engine, atau database explorer.

Produk menggunakan shared Rust database core yang digunakan oleh:

- Tauri Desktop
- Web Backend
- future CLI
- future Database Agent
- future self-hosted deployment

---

# 3. Product Goals

Tujuan utama MVP:

1. Membuat database client yang cepat dan ringan.
2. Mendukung PostgreSQL dan MySQL.
3. Memberikan SQL editor yang nyaman digunakan.
4. Memberikan database object explorer yang lengkap.
5. Memungkinkan inspeksi schema, table, view, sequence, index, dan constraint.
6. Menyediakan result grid untuk query.
7. Mendukung desktop dan web dari satu codebase semaksimal mungkin.
8. Menggunakan database core yang sama di seluruh platform.
9. Memiliki arsitektur aman untuk menyimpan dan menggunakan database credential.
10. Menyediakan fondasi untuk menambahkan database engine lain.

---

# 4. Non-Goals MVP

Jangan implementasikan fitur berikut pada MVP kecuali dibutuhkan oleh arsitektur dasar:

- Oracle
- SQL Server
- MongoDB
- Cassandra
- Redis browser
- SAML
- Enterprise SSO
- Kubernetes
- Plugin marketplace
- AI SQL assistant
- Collaborative SQL editor
- Database debugger
- Stored procedure debugger
- Advanced schema migration engine
- Git integration
- Multi-region infrastructure
- Full DataGrip-compatible SQL language engine
- Advanced database monitoring
- DBA administration suite

Arsitektur boleh mempersiapkan kemampuan tersebut tetapi jangan mengimplementasikannya sekarang.

---

# 5. Target Platforms

## Desktop

Target OS:

- Windows
- macOS
- Linux

Technology:

- Tauri 2
- Rust
- React
- TypeScript

Desktop harus dapat mengakses database secara langsung dari komputer pengguna.

Contoh:

Desktop App → PostgreSQL

Desktop App → MySQL

Desktop App → SSH Tunnel → Database

---

## Web

Technology:

Frontend:

- React
- TypeScript
- Vite

Backend:

- Rust
- Axum
- Tokio

Web browser tidak boleh terhubung langsung ke database.

Arsitektur:

Browser

→ HTTPS / WebSocket

→ Rust Backend

→ Shared Database Core

→ PostgreSQL / MySQL

---

# 6. High-Level Architecture

Gunakan struktur:

```text
React UI
    │
    ├── Desktop Transport
    │       │
    │       ▼
    │    Tauri IPC
    │       │
    │       ▼
    │    Rust Host
    │
    └── Web Transport
            │
            ▼
        HTTP / WebSocket
            │
            ▼
        Axum Backend

                │
                ▼

          Shared db-core

                │

        ┌───────┴───────┐
        │               │
        ▼               ▼

   PostgreSQL         MySQL
     Adapter          Adapter
```

Database core tidak boleh bergantung pada:

- React
- Tauri
- Axum
- HTTP
- WebSocket
- browser

Database core harus dapat digunakan sebagai library Rust independen.

---

# 7. Repository Structure

Gunakan monorepo.

Target struktur:

```text
database-studio/

apps/
    desktop/
    web/

services/
    api/
    query-worker/

crates/
    db-core/
    db-types/
    db-postgres/
    db-mysql/
    db-metadata/
    db-query/
    db-security/
    sql-language/
    workspace-core/

packages/
    ui/
    editor/
    db-explorer/
    result-grid/
    shared-types/

infra/
    docker/
```

Struktur boleh disesuaikan jika terdapat alasan teknis yang jelas.

---

# 8. Core Engineering Principles

Gunakan prinsip berikut selama implementasi.

## 8.1 Shared Core

Semua operasi database utama harus berada di Rust shared core.

Termasuk:

- connection management
- query execution
- query cancellation
- transaction management
- metadata introspection
- object discovery
- capability detection
- result streaming

---

## 8.2 Adapter Architecture

Setiap database menggunakan adapter.

Contoh:

```text
DatabaseAdapter

├── PostgresAdapter
├── MySqlAdapter
└── future adapters
```

Adapter tidak boleh memaksa seluruh database memiliki fitur yang sama.

Gunakan capability-based architecture.

Contoh capability:

```rust
pub struct DatabaseCapabilities {
    pub schemas: bool,
    pub views: bool,
    pub materialized_views: bool,
    pub sequences: bool,
    pub procedures: bool,
    pub functions: bool,
    pub triggers: bool,
    pub extensions: bool,
    pub query_cancel: bool,
    pub explain: bool,
}
```

---

# 9. Database Adapter Interface

Buat trait dasar dengan konsep kurang lebih:

```rust
#[async_trait]
pub trait DatabaseAdapter: Send + Sync {
    fn database_type(&self) -> DatabaseType;

    fn capabilities(&self) -> DatabaseCapabilities;

    async fn connect(
        &self,
        config: ConnectionConfig,
    ) -> DbResult<DbSession>;

    async fn introspect(
        &self,
        session: &DbSession,
        request: MetadataRequest,
    ) -> DbResult<MetadataResponse>;

    async fn execute(
        &self,
        session: &DbSession,
        request: QueryRequest,
    ) -> DbResult<QueryStream>;

    async fn cancel(
        &self,
        query_id: QueryId,
    ) -> DbResult<()>;
}
```

Trait boleh berkembang selama implementasi.

Jangan membuat abstraksi yang menghilangkan fitur spesifik PostgreSQL atau MySQL.

---

# 10. Supported Database Engines

MVP harus mendukung:

## PostgreSQL

Minimum object support:

- database
- schema
- table
- column
- primary key
- foreign key
- unique constraint
- index
- view
- materialized view
- sequence
- function
- procedure
- trigger
- enum / custom type

---

## MySQL

Minimum object support:

- database
- table
- column
- primary key
- foreign key
- unique constraint
- index
- view
- function
- procedure
- trigger

MySQL tidak perlu dipaksa memiliki object type seperti PostgreSQL sequence.

---

# 11. Connection Manager

User harus dapat:

- membuat connection
- mengedit connection
- menghapus connection
- melakukan test connection
- connect
- disconnect
- reconnect

Connection fields minimum:

```text
name
database type
host
port
username
password
database
SSL mode
```

Optional:

```text
SSH tunnel
connection timeout
query timeout
```

---

# 12. Credential Security

## Desktop

Database password tidak boleh disimpan plaintext dalam:

- JSON
- SQLite
- config file
- frontend storage

Gunakan OS credential storage.

SQLite hanya menyimpan metadata connection dan secret reference.

---

## Web

Database credential tidak boleh dikirim kembali ke browser setelah tersimpan.

Credential harus:

- dienkripsi
- dikelola oleh backend
- hanya didekripsi ketika database connection dibuat

Arsitektur harus siap menggunakan:

- KMS
- Secret Manager
- Vault

Implementasi lokal development dapat menggunakan abstraction yang lebih sederhana tetapi API secrets harus dirancang agar dapat diganti dengan production secret store.

---

# 13. Database Explorer

Sidebar database explorer harus menggunakan tree structure.

Contoh PostgreSQL:

```text
Connection
└── Database
    └── Schema
        ├── Tables
        ├── Views
        ├── Materialized Views
        ├── Sequences
        ├── Functions
        ├── Procedures
        └── Types
```

Contoh MySQL:

```text
Connection
└── Database
    ├── Tables
    ├── Views
    ├── Procedures
    ├── Functions
    └── Triggers
```

Gunakan lazy loading.

Jangan mengambil semua metadata ketika connection pertama dibuka.

Metadata baru diambil ketika node dibuka atau dibutuhkan.

---

# 14. Table Inspector

Ketika table dipilih, tampilkan detail:

- columns
- data types
- nullable
- default value
- primary key
- foreign keys
- indexes
- constraints

Tabs minimum:

```text
Data
Columns
Indexes
Constraints
Foreign Keys
DDL
```

---

# 15. Table Data Viewer

User harus dapat:

- melihat isi table
- memilih jumlah rows
- sorting
- filtering dasar
- refresh

Default fetch:

```text
100 rows
```

Jangan load seluruh table.

Gunakan pagination atau lazy data fetching.

---

# 16. SQL Editor

Gunakan Monaco Editor.

Minimum features MVP:

- syntax highlighting
- multiple tabs
- execute current statement
- execute selected SQL
- execute full editor
- keyboard shortcut
- SQL formatting dasar
- query errors
- query execution time
- affected rows
- query cancel
- autocomplete keywords
- autocomplete table names
- autocomplete column names

Support:

```text
PostgreSQL dialect
MySQL dialect
```

---

# 17. SQL Editor Sessions

Setiap SQL editor harus mempunyai session identifier.

Contoh:

```text
editor_session_id
```

Untuk query yang memerlukan session persistence, gunakan database connection yang sama.

Harus mendukung pola:

```sql
BEGIN;

SET search_path = demo;

CREATE TEMP TABLE test (...);

SELECT * FROM test;

COMMIT;
```

Semantics tersebut tidak boleh rusak karena setiap statement memakai random connection dari pool.

---

# 18. Result Grid

Result viewer minimum harus memiliki:

- virtualized rows
- virtualized columns jika dibutuhkan
- column header
- null visualization
- sortable values
- row selection
- cell selection
- copy cell
- copy rows
- copy as CSV
- execution information

Jangan render ribuan DOM rows sekaligus.

---

# 19. Query Execution

Query execution harus asynchronous.

Lifecycle:

```text
Created
Running
Streaming
Completed
Failed
Cancelled
```

Setiap query memiliki:

```text
query_id
session_id
connection_id
started_at
status
```

---

# 20. Query Streaming

Jangan mengumpulkan semua rows terlebih dahulu sebelum dikirim ke frontend.

Gunakan batch streaming.

Contoh event:

```json
{
  "type": "rows",
  "queryId": "query-123",
  "batch": 1,
  "rows": []
}
```

Event types minimum:

```text
query_started
columns
rows
query_completed
query_failed
query_cancelled
```

---

# 21. Desktop Transport

Desktop frontend berkomunikasi dengan backend menggunakan Tauri IPC.

Buat abstraction:

```typescript
interface DatabaseTransport {
    execute(
        request: QueryRequest
    ): AsyncIterable<QueryEvent>;

    cancel(
        queryId: string
    ): Promise<void>;

    metadata(
        request: MetadataRequest
    ): Promise<MetadataResponse>;
}
```

Implementasi:

```text
TauriDatabaseTransport
```

---

# 22. Web Transport

Gunakan interface yang sama.

Implementasi:

```text
HttpDatabaseTransport
```

Gunakan:

HTTP:

- connection CRUD
- metadata
- query creation
- query cancellation

WebSocket:

- query result streaming
- query execution events

Frontend tidak boleh mengetahui detail database driver.

---

# 23. Web Authentication

Gunakan:

```text
OAuth 2
OIDC
```

Target provider awal:

- GitHub
- Google

Arsitektur harus memungkinkan provider lain ditambahkan.

Gunakan server-side session.

Browser menggunakan:

```text
Secure
HttpOnly
SameSite cookie
```

Jangan menyimpan main authentication token di localStorage.

---

# 24. Authorization

Gunakan resource-based permissions.

Minimum roles:

```text
Owner
Admin
Developer
Viewer
```

Minimum permissions:

```text
workspace:manage

connection:create
connection:update
connection:delete
connection:read

query:execute

metadata:read

secret:manage
```

Implementasikan authorization di backend.

Frontend permission check hanya untuk UX dan bukan security boundary.

---

# 25. Organization and Workspace

Web version harus memiliki konsep:

```text
User
Organization
Workspace
Connection
```

Hierarchy:

```text
Organization
    │
    └── Workspace
            │
            ├── Connections
            ├── Saved Queries
            └── Members
```

---

# 26. Control Plane Database

Gunakan PostgreSQL sebagai application database.

Minimum entities:

```text
users
organizations
organization_members
workspaces
workspace_members
connections
saved_queries
query_history
sessions
audit_events
```

Database credential harus disimpan di secret store abstraction dan bukan langsung plaintext di table connections.

---

# 27. Multi-Tenant Isolation

Semua web resources harus scoped minimal dengan:

```text
organization_id
workspace_id
```

Jangan mengambil resource berdasarkan ID saja tanpa tenant scope.

Contoh benar:

```sql
SELECT *
FROM connections
WHERE organization_id = $1
AND id = $2;
```

Cross-tenant access harus dianggap critical security issue.

---

# 28. Query History

Simpan metadata:

```text
query_id
user_id
connection_id
query_text
status
duration
rows_returned
started_at
finished_at
```

User harus dapat:

- melihat query sebelumnya
- membuka kembali query
- menjalankan ulang query

Tambahkan setting untuk menonaktifkan query history pada connection tertentu.

---

# 29. Saved Queries

User dapat:

- save query
- edit saved query
- delete saved query
- rename query

Web:

Saved query terkait dengan workspace.

Desktop:

Saved query disimpan lokal.

---

# 30. Local Desktop Storage

Gunakan SQLite.

Digunakan untuk:

- connection metadata
- settings
- query history
- workspace state
- recent files
- saved queries

Secret tidak boleh disimpan plaintext di SQLite.

---

# 31. Frontend State Architecture

Gunakan:

```text
React
TypeScript
TanStack Query
Zustand
```

TanStack Query:

- backend/server state
- metadata
- connection state
- saved query list

Zustand:

- UI state
- active tabs
- panel sizes
- selected object
- editor states

---

# 32. UI Layout

Minimum layout:

```text
┌──────────────────────────────────────────────┐
│ Toolbar                                      │
├─────────────┬────────────────────────────────┤
│             │                                │
│ DB Explorer │ SQL Editor                     │
│             │                                │
│             ├────────────────────────────────┤
│             │ Result / Messages / Explain    │
│             │                                │
├─────────────┴────────────────────────────────┤
│ Status Bar                                   │
└──────────────────────────────────────────────┘
```

Panels harus resizable.

---

# 33. Tabs

Support multiple tabs:

```text
SQL Editor
Table Data
Table Definition
View
Function
```

Tab state tidak boleh hilang ketika berpindah tab.

SQL editor harus mempertahankan:

- text
- cursor
- selections
- undo history

sebisa mungkin.

---

# 34. Query Cancellation

User harus dapat membatalkan query yang sedang berjalan.

UI:

```text
Run
Cancel
```

Cancel harus benar-benar meminta backend / database driver menghentikan query.

Jangan hanya menghentikan loading indicator.

---

# 35. Transactions

SQL editor harus mendukung:

```text
Auto Commit ON
Auto Commit OFF
```

Ketika auto commit OFF:

```text
Commit
Rollback
```

harus tersedia.

Transaction state harus terkait dengan editor/database session.

---

# 36. Explain Query

MVP harus mendukung basic EXPLAIN.

PostgreSQL:

```sql
EXPLAIN
```

MySQL:

```sql
EXPLAIN
```

Output dapat ditampilkan sebagai table/text terlebih dahulu.

Graph visualization bukan requirement MVP.

---

# 37. Error Handling

Backend harus memiliki unified error model.

Contoh:

```rust
DbError {
    code,
    category,
    message,
    database_code,
    details,
}
```

Frontend harus dapat membedakan:

```text
authentication error
connection error
syntax error
timeout
cancelled
permission error
network error
database error
```

Jangan expose internal stack trace kepada end user.

---

# 38. Logging

Rust backend gunakan structured logging.

Gunakan:

```text
tracing
OpenTelemetry-ready architecture
```

Jangan log:

- database password
- access token
- private key

SQL logging harus configurable.

---

# 39. Security Requirements

Minimum security rules:

1. Never store plaintext database passwords.
2. Never expose database credentials to browser after storage.
3. Backend validates workspace ownership.
4. Backend validates connection ownership.
5. Every web API endpoint requiring user data must authenticate.
6. Every protected resource must authorize.
7. Rate limit sensitive endpoints.
8. Do not expose arbitrary OS filesystem access.
9. Restrict Tauri permissions.
10. Do not expose shell command execution to frontend.
11. Use TLS in production.
12. Secrets must never appear in logs.

---

# 40. Performance Requirements

Initial engineering targets:

Cold desktop startup:

```text
< 2 seconds target
```

Idle memory:

```text
< 200 MB target
```

Database tree:

```text
10,000+ objects should remain navigable
```

Result viewer:

```text
100,000+ returned rows should not freeze UI
```

Do not keep unlimited query result data in frontend memory.

Use bounded buffering.

These numbers are engineering goals and must be benchmarked rather than treated as guaranteed performance.

---

# 41. Query Safety

Do not automatically modify SQL submitted by users.

Database Studio merupakan database IDE sehingga arbitrary SQL execution adalah bagian dari produk.

Tetapi pastikan:

```text
User SQL
```

hanya dieksekusi pada:

```text
connection selected by authorized user
```

dan tidak pernah dapat diarahkan ke control-plane database karena kesalahan routing.

Pisahkan customer database execution dengan application database.

---

# 42. Query Worker

Web architecture harus memungkinkan query execution dipisahkan dari API process.

MVP boleh memulai dengan:

```text
API + Query Engine
```

dalam satu service.

Tetapi design interface agar nantinya dapat menjadi:

```text
API
 │
 ▼
Query Queue
 │
 ▼
Query Worker
 │
 ▼
Customer Database
```

tanpa perubahan besar pada frontend protocol.

---

# 43. Deployment MVP

Web:

```text
Docker
```

Deployment target dapat menggunakan managed container platform.

MVP tidak membutuhkan Kubernetes.

Services minimum:

```text
web frontend
backend API
PostgreSQL
Redis optional
```

---

# 44. Desktop Distribution

Future release pipeline harus mendukung:

Windows:

```text
MSI / EXE
```

macOS:

```text
DMG
```

Linux:

```text
AppImage
DEB
```

Code signing dan auto-update boleh menjadi fase setelah application core stabil.

---

# 45. Testing Strategy

Minimum tests:

## Rust

Unit tests:

```text
db-core
metadata mapping
query lifecycle
capabilities
error mapping
```

Integration tests:

```text
PostgreSQL adapter
MySQL adapter
```

Gunakan database Docker untuk integration test.

---

## Frontend

Tests minimum:

```text
database tree
query execution state
tab state
result grid
connection forms
```

---

## E2E

Minimum flow:

```text
create connection
test connection
connect
open database
open table
run SELECT
view result
cancel query
disconnect
```

---

# 46. Developer Environment

Sediakan Docker Compose.

Minimum:

```text
PostgreSQL
MySQL
application PostgreSQL
```

Command target:

```bash
docker compose up -d
```

Kemudian developer harus dapat menjalankan:

```bash
pnpm dev
```

dan:

```bash
cargo run
```

dengan setup seminimal mungkin.

---

# 47. Recommended Technology Stack

Frontend:

```text
React
TypeScript
Vite
Monaco Editor
TanStack Query
Zustand
TanStack Virtual
React Resizable Panels
```

Desktop:

```text
Tauri 2
Rust
```

Backend:

```text
Rust
Axum
Tokio
Tower
tower-http
```

Database:

```text
SQLx
PostgreSQL
MySQL
SQLite
```

Infrastructure:

```text
Docker
GitHub Actions
```

Observability:

```text
tracing
OpenTelemetry-ready
```

---

# 48. Development Phases

## Phase 1 — Foundation

Implement:

```text
monorepo
shared types
db-core
database adapter trait
PostgreSQL adapter
MySQL adapter
connection manager
query engine
metadata engine
```

Success criteria:

Rust core dapat:

- connect PostgreSQL
- connect MySQL
- list schemas/databases
- list tables
- inspect table
- execute SELECT
- stream rows
- cancel query

---

# 49. Phase 2 — Desktop MVP

Implement:

```text
Tauri
React shell
connection manager
database explorer
SQL editor
result grid
table viewer
query history
transactions
```

Success criteria:

User dapat menggunakan desktop app untuk pekerjaan database dasar sehari-hari.

---

# 50. Phase 3 — Web MVP

Implement:

```text
Axum API
authentication
session
organization
workspace
authorization
secure connection storage
WebSocket result streaming
```

Success criteria:

User dapat login dan menjalankan query terhadap database yang telah dikonfigurasi melalui browser.

---

# 51. Phase 4 — Productivity

Implement:

```text
better autocomplete
saved queries
EXPLAIN
query history search
DDL viewer
table editor
data editing
keyboard shortcuts
```

---

# 52. Phase 5 — Advanced Database IDE

Future scope:

```text
schema diff
schema migration
ER diagram
DDL generator
refactoring
advanced autocomplete
go to definition
dependency graph
database agent
SSH tunnel
enterprise auth
self hosted
```

---

# 53. Definition of MVP Done

MVP dianggap selesai jika user dapat melakukan flow berikut.

## Desktop

```text
Launch application

→ Add PostgreSQL/MySQL connection

→ Test connection

→ Connect

→ Browse database objects

→ Open table

→ Inspect schema

→ View table data

→ Create SQL tab

→ Write SQL

→ Execute SQL

→ See streamed result

→ Cancel query

→ Use transaction

→ View query history
```

---

## Web

```text
Open application

→ Login

→ Create workspace

→ Add database connection

→ Save connection securely

→ Browse database

→ Open SQL editor

→ Execute query

→ Receive result stream

→ Cancel query

→ View history
```

---

# 54. Coding Rules for Codex

When implementing this project:

1. Prefer simple architecture before introducing abstractions.
2. Do not introduce microservices without clear need.
3. Keep db-core independent from UI and network layers.
4. Avoid database-specific logic in shared UI.
5. Put database-specific behavior inside adapters.
6. Prefer explicit Rust types over loosely typed maps.
7. Avoid unwrap() in production Rust paths.
8. Return typed errors.
9. Do not expose secrets through logs.
10. Do not store secrets in frontend state longer than necessary.
11. Write tests for database adapters.
12. Do not prematurely optimize without profiling.
13. Keep modules small and responsibility-specific.
14. Document architectural decisions when implementation differs from this PRD.
15. Keep dependency count reasonable.
16. Use stable Rust unless a feature explicitly requires nightly.

---

# 55. Initial Codex Task

Start implementation with Phase 1 only.

Do not start the desktop UI before the shared database core is usable.

Initial tasks:

1. Initialize monorepo.
2. Initialize Rust workspace.
3. Create `db-types`.
4. Create `db-core`.
5. Define connection configuration types.
6. Define database capability types.
7. Define database object types.
8. Define query request/response types.
9. Define DatabaseAdapter trait.
10. Implement PostgreSQL adapter.
11. Implement MySQL adapter.
12. Implement metadata introspection.
13. Implement query execution.
14. Implement query cancellation.
15. Implement query result streaming.
16. Create PostgreSQL and MySQL integration tests.
17. Add Docker Compose development databases.
18. Add README explaining architecture and development setup.

Do not implement speculative features outside Phase 1 until Phase 1 acceptance criteria pass.

---

# 56. Phase 1 Acceptance Criteria

The following commands/workflows must work.

PostgreSQL:

```text
connect
list databases/schemas
list tables
list views
list sequences
inspect columns
inspect indexes
inspect constraints
execute query
stream rows
cancel running query
disconnect
```

MySQL:

```text
connect
list databases
list tables
list views
inspect columns
inspect indexes
inspect constraints
execute query
stream rows
cancel running query
disconnect
```

Automated integration tests must verify both database engines.

---

# 57. Final Architecture Principle

The project should always preserve this dependency direction:

```text
UI
 ↓
Transport
 ↓
Application Layer
 ↓
Database Core
 ↓
Database Adapter
 ↓
Database Driver
```

Never invert the dependency so that:

```text
db-core
```

depends on:

```text
Tauri
Axum
React
HTTP
WebSocket
```

The database core is the long-term foundation of the product.

All future platforms should reuse it.
