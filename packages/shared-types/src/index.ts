export type Engine = 'Postgres' | 'MySql';
export type Transaction = 'Idle' | 'Active' | 'Failed' | 'Unknown';
export type Status =
  | 'Created'
  | 'Running'
  | 'Streaming'
  | 'Completed'
  | 'Failed'
  | 'Cancelled';
export const terminal = (status: Status) =>
  ['Completed', 'Failed', 'Cancelled'].includes(status);
export interface DbError {
  code: string;
  category: string;
  message: string;
  database_code?: string | null;
}
export interface Profile {
  id: string;
  revision: number;
  name: string;
  engine: Engine;
  host: string;
  port: number;
  username: string;
  database: string;
  ssl: string;
  connect_timeout_ms: number;
  query_timeout_ms: number | null;
  history_enabled: boolean;
  restore_sql: boolean;
}
export interface Session {
  id: string;
  connection_id: string;
  revision: number;
  auto_commit: boolean;
  transaction: Transaction;
  valid: boolean;
  busy: boolean;
}
export type ObjectKind =
  | 'Database'
  | 'Schema'
  | 'Table'
  | 'View'
  | 'MaterializedView'
  | 'Sequence'
  | 'Function'
  | 'Procedure'
  | 'Trigger'
  | 'CustomType';
export interface ObjectId {
  engine: Engine;
  database: string;
  schema: string | null;
  name: string;
  kind: ObjectKind;
}
export interface Column {
  name: string;
  database_type: string;
}
export interface ColumnMetadata {
  name: string;
  data_type: string;
  nullable: boolean;
  default: string | null;
  ordinal: number;
}
export interface Metadata {
  objects: ObjectId[];
  columns: ColumnMetadata[];
  indexes: { name: string; definition: string; unique: boolean }[];
  constraints: { name: string; kind: string; definition: string }[];
  ddl: string | null;
  editing?: {
    primary_key: string[];
    generated: string[];
    transactional: boolean;
    warnings: string[];
  } | null;
}
export interface Cell {
  kind: string;
  value: string | null;
}
export type Row = Cell[];
export interface Result {
  id: string;
  statement: number;
  columns: Column[];
  rows: number;
  terminal: QueryTerminal | null;
}
export interface QueryTerminal {
  type: string;
  summary?: {
    results_complete: boolean;
    affected_rows: string;
    rows_returned: string;
  };
  error?: DbError;
  outcome?: string;
  session_valid?: boolean;
  elapsed_ms: number;
}
export interface Execution {
  id: string;
  session_id: string;
  connection_id: string;
  status: Status;
  results: Result[];
  error: DbError | null;
  started_at_ms: number;
  finished_at_ms: number | null;
  persistence_error: DbError | null;
}
export interface Document {
  id: string;
  connection_id: string;
  kind: 'saved' | 'history' | 'editor';
  name: string;
  content: { sql: string; execution?: Execution; cursor?: number };
  created_at_ms: number;
}
export interface Notification {
  execution_id: string;
  sequence: number;
  status: Status;
  rows: number;
}
export interface DatabaseTransport {
  capabilities?: {
    platform: 'desktop' | 'web';
    manageConnections: boolean;
    execute: boolean;
    persistentCredentials: boolean;
  };

  onCloseRequested?: (handler: () => Promise<boolean>) => Promise<() => void>;
  request<T>(operation: string, args?: Record<string, unknown>): Promise<T>;
  watch(
    executionId: string,
    onEvent: (event: Notification) => void,
  ): Promise<() => void>;
}
export function describeError(error: unknown): string {
  if (typeof error === 'object' && error && 'message' in error)
    return String(error.message);
  return typeof error === 'string'
    ? error
    : 'Operasi gagal. Periksa koneksi dan coba kembali.';
}

export type DocumentHeader = Omit<Document, 'content'> & {
  revision?: number;
  can_edit?: boolean;
  favorite?: boolean;
};
