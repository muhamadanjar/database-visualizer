import { useEffect, useMemo, useRef, useState } from 'react';
import type {
  DatabaseTransport,
  Engine,
  Execution,
  ObjectId,
  SchemaChangeRequest,
  SchemaColumn,
  SchemaConstraint,
  SchemaLiteral,
  SchemaOperation,
  SchemaPreview,
} from '@database-studio/contracts';
import { describeError, terminal } from '@database-studio/contracts';

export interface SchemaManagerProps {
  transport: DatabaseTransport;
  connectionRevision: number;
  connectionId: string;
  table?: ObjectId;
  createContext?: { engine: Engine; database: string; schema?: string | null };
  onApplied?: (
    operation: SchemaOperation,
    target?: ObjectId,
  ) => void | Promise<void>;
  onBusyChange?: (busy: boolean) => void;
}
type OperationType = SchemaOperation['type'];
type ConstraintKind = 'primary_key' | 'unique' | 'foreign_key' | 'check';
type DefaultKind = 'none' | 'text' | 'signed' | 'boolean' | 'null';
interface ColumnDraft {
  name: string;
  dataType: string;
  nullable: boolean;
  defaultKind: DefaultKind;
  defaultValue: string;
}
const labels: Record<OperationType, string> = {
  create_table: 'Buat tabel',
  rename_table: 'Ubah nama tabel',
  drop_table: 'Hapus tabel',
  add_column: 'Tambah kolom',
  alter_column: 'Ubah kolom',
  rename_column: 'Ubah nama kolom',
  drop_column: 'Hapus kolom',
  create_index: 'Buat indeks',
  drop_index: 'Hapus indeks',
  add_constraint: 'Tambah constraint',
  drop_constraint: 'Hapus constraint',
};
const types = Object.keys(labels) as OperationType[];
const emptyColumn = (): ColumnDraft => ({
  name: '',
  dataType: 'TEXT',
  nullable: true,
  defaultKind: 'none',
  defaultValue: '',
});
const csv = (value: string) =>
  value
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean);
const toLiteral = (
  kind: DefaultKind,
  value: string,
): SchemaLiteral | undefined =>
  kind === 'none'
    ? undefined
    : kind === 'null'
      ? { type: 'null', value: null }
      : kind === 'boolean'
        ? { type: 'boolean', value: value === 'true' }
        : kind === 'signed'
          ? { type: 'decimal', value }
          : { type: 'text', value };
const toColumn = (draft: ColumnDraft): SchemaColumn => ({
  name: draft.name,
  data_type: draft.dataType,
  nullable: draft.nullable,
  default: toLiteral(draft.defaultKind, draft.defaultValue),
});

export function SchemaManager({
  transport,
  connectionRevision,
  connectionId,
  table,
  createContext,
  onApplied,
  onBusyChange,
}: SchemaManagerProps) {
  const currentRevision = useRef(connectionRevision);
  currentRevision.current = connectionRevision;
  const [type, setType] = useState<OperationType>(
    table ? 'add_column' : 'create_table',
  );
  const [tableName, setTableName] = useState('new_table');
  const [schema, setSchema] = useState(
    table?.schema ?? createContext?.schema ?? '',
  );
  const [columns, setColumns] = useState<ColumnDraft[]>([emptyColumn()]);
  const [column, setColumn] = useState<ColumnDraft>(emptyColumn());
  const [name, setName] = useState('');
  const [newName, setNewName] = useState('');
  const [indexName, setIndexName] = useState('');
  const [indexColumns, setIndexColumns] = useState('');
  const [unique, setUnique] = useState(false);
  const [constraintKind, setConstraintKind] =
    useState<ConstraintKind>('primary_key');
  const [constraintName, setConstraintName] = useState('');
  const [constraintColumns, setConstraintColumns] = useState('');
  const [referencedSchema, setReferencedSchema] = useState(
    table?.schema ?? createContext?.schema ?? '',
  );
  const [referencedTable, setReferencedTable] = useState('');
  const [referencedColumns, setReferencedColumns] = useState('');
  const [checkExpression, setCheckExpression] = useState('');
  const [preview, setPreview] = useState<SchemaPreview | null>(null);
  const [typedName, setTypedName] = useState('');
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState('');
  const [outcomeUnknown, setOutcomeUnknown] = useState(false);
  const [unknownExecutionId, setUnknownExecutionId] = useState<string | null>(
    null,
  );
  const [busy, setBusy] = useState(false);
  const onBusyChangeRef = useRef(onBusyChange);
  onBusyChangeRef.current = onBusyChange;
  const allowedTypes = table ? types : (['create_table'] as OperationType[]);
  useEffect(() => {
    if (!table && type !== 'create_table') setType('create_table');
  }, [table, type]);
  useEffect(() => {
    onBusyChangeRef.current?.(busy || outcomeUnknown);
  }, [busy, outcomeUnknown]);

  const createEngine = table?.engine ?? createContext?.engine;
  const requestTable = useMemo<ObjectId>(() => {
    const base = table ?? {
      engine: createContext?.engine ?? 'Postgres',
      database: createContext?.database ?? '',
      schema: createContext?.schema ?? null,
      name: tableName,
      kind: 'Table' as const,
    };
    return {
      ...base,
      name: type === 'create_table' ? tableName : base.name,
      schema:
        type === 'create_table'
          ? createEngine === 'MySql'
            ? null
            : schema || null
          : base.schema,
    };
  }, [createContext, createEngine, schema, table, tableName, type]);
  const operation = useMemo<SchemaOperation>(() => {
    switch (type) {
      case 'create_table':
        return { type, columns: columns.map(toColumn) };
      case 'rename_table':
        return { type, new_name: newName };
      case 'drop_table':
        return { type };
      case 'add_column':
        return { type, column: toColumn(column) };
      case 'alter_column':
        return { type, column: toColumn({ ...column, name }) };
      case 'rename_column':
        return { type, name, new_name: newName };
      case 'drop_column':
        return { type, name };
      case 'create_index':
        return {
          type,
          index: { name: indexName, columns: csv(indexColumns), unique },
        };
      case 'drop_index':
        return { type, name };
      case 'drop_constraint':
        return {
          type,
          name:
            constraintKind === 'primary_key' && requestTable.engine === 'MySql'
              ? 'PRIMARY'
              : name,
          kind: constraintKind,
        };
      case 'add_constraint': {
        let constraint: SchemaConstraint;
        if (constraintKind === 'check')
          constraint = {
            kind: constraintKind,
            name: constraintName,
            expression: checkExpression,
          };
        else if (constraintKind === 'foreign_key')
          constraint = {
            kind: constraintKind,
            name: constraintName,
            columns: csv(constraintColumns),
            referenced_table: {
              ...requestTable,
              schema: referencedSchema || null,
              name: referencedTable,
              kind: 'Table',
            },
            referenced_columns: csv(referencedColumns),
          };
        else
          constraint = {
            kind: constraintKind,
            name: constraintName,
            columns: csv(constraintColumns),
          };
        return { type, constraint };
      }
    }
  }, [
    checkExpression,
    column,
    columns,
    constraintColumns,
    constraintKind,
    constraintName,
    indexColumns,
    indexName,
    name,
    newName,
    referencedColumns,
    referencedSchema,
    referencedTable,
    requestTable,
    type,
    unique,
  ]);
  const fingerprint = JSON.stringify([connectionId, requestTable, operation]);
  useEffect(() => {
    setPreview(null);
    setTypedName('');
    setConfirmed(false);
    setOutcomeUnknown(false);
    setUnknownExecutionId(null);
  }, [fingerprint]);
  useEffect(() => {
    setPreview(null);
    setTypedName('');
    setConfirmed(false);
  }, [connectionRevision]);

  const previewSql = async () => {
    if (type !== 'create_table' && !table) {
      setError('Pilih tabel yang sudah ada untuk operasi ini.');
      return;
    }
    const requestedRevision = connectionRevision;
    setBusy(true);
    setError('');
    try {
      const value = await transport.request<SchemaPreview>('schema_preview', {
        connection_id: connectionId,
        request: { table: requestTable, operation },
      });
      if (
        value.profile_revision !== requestedRevision ||
        currentRevision.current !== requestedRevision
      ) {
        setPreview(null);
        setError('Profil koneksi berubah; buat Preview schema baru.');
        return;
      }
      setPreview(value);
    } catch (e) {
      setError(describeError(e));
    } finally {
      setBusy(false);
    }
  };

  const observe = async (executionId: string) => {
    let execution: Execution;
    try {
      for (;;) {
        execution = await transport.request<Execution>('execution', {
          execution_id: executionId,
        });
        if (terminal(execution.status)) break;
        await new Promise<void>((resolve) => setTimeout(resolve, 500));
      }
    } catch {
      setOutcomeUnknown(true);
      setUnknownExecutionId(executionId);
      setError(
        `DDL dikirim tetapi hasil akhirnya tidak diketahui. Periksa database dan execution ${executionId}; SQL tidak dijalankan ulang.`,
      );
      return;
    }
    if (
      execution.error?.code === 'QUERY_OUTCOME_UNKNOWN' ||
      execution.results.some((result) => result.terminal?.outcome === 'Unknown')
    ) {
      setOutcomeUnknown(true);
      setUnknownExecutionId(executionId);
      setError(
        `Hasil DDL tidak diketahui. Periksa database dan execution ${executionId}; SQL tidak dijalankan ulang.`,
      );
      return;
    }
    try {
      await transport.request('close_execution', { execution_id: executionId });
    } catch (e) {
      setError(
        `DDL selesai, tetapi hasil execution tidak dapat ditutup: ${describeError(e)}`,
      );
    }
    if (execution.status === 'Completed') {
      setPreview(null);
      setTypedName('');
      setConfirmed(false);
      setUnknownExecutionId(null);
      try {
        await onApplied?.(operation, requestTable);
      } catch (e) {
        setError(
          `DDL selesai, tetapi refresh Inspector gagal: ${describeError(e)}`,
        );
      }
      return;
    }
    setError(
      execution.error?.message ??
        `DDL berakhir dengan status ${execution.status}.`,
    );
  };

  const apply = async () => {
    if (!table && type !== 'create_table') return;
    if (
      outcomeUnknown ||
      !preview ||
      (preview.destructive &&
        (!confirmed || typedName !== preview.affected_name))
    )
      return;
    setBusy(true);
    setError('');
    const request: SchemaChangeRequest = {
      table: requestTable,
      operation,
      ...(preview.destructive
        ? { confirmation: { confirmed, typed_name: typedName } }
        : {}),
    };
    try {
      const result = await transport.request<{ execution_id: string }>(
        'schema_apply',
        {
          connection_id: connectionId,
          expected_profile_revision: preview.profile_revision,
          request,
        },
      );
      await observe(result.execution_id);
    } catch (e) {
      if (
        typeof e === 'object' &&
        e !== null &&
        'code' in e &&
        e.code === 'SCHEMA_PROFILE_CHANGED'
      ) {
        setPreview(null);
        setTypedName('');
        setConfirmed(false);
        setError(describeError(e));
        return;
      }
      setOutcomeUnknown(true);
      setUnknownExecutionId(null);
      setError(
        `Respons Apply DDL tidak diterima dengan pasti. Periksa database sebelum melanjutkan; jangan kirim ulang. ${describeError(e)}`,
      );
    } finally {
      setBusy(false);
    }
  };

  const acknowledgeUnknown = () => {
    if (
      !window.confirm(
        'Saya sudah memeriksa schema aktual dan memastikan operasi DDL tidak lagi berjalan. Apply tidak akan diulang otomatis.',
      )
    )
      return;
    if (unknownExecutionId) {
      void transport
        .request('close_execution', { execution_id: unknownExecutionId })
        .catch(() => {});
    }
    setOutcomeUnknown(false);
    setUnknownExecutionId(null);
    setPreview(null);
    setTypedName('');
    setConfirmed(false);
    setError('');
  };
  const editor = (
    draft: ColumnDraft,
    update: (value: Partial<ColumnDraft>) => void,
    prefix: string,
  ) => (
    <fieldset className="schema-fieldset">
      <legend>{prefix}</legend>
      <label>
        Nama kolom
        <input
          aria-label={`${prefix} nama`}
          value={draft.name}
          onChange={(e) => update({ name: e.target.value })}
        />
      </label>
      <label>
        Tipe data
        <input
          aria-label={`${prefix} tipe`}
          value={draft.dataType}
          onChange={(e) => update({ dataType: e.target.value })}
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={draft.nullable}
          onChange={(e) => update({ nullable: e.target.checked })}
        />{' '}
        Nullable
      </label>
      <label>
        Jenis default
        <select
          aria-label={`${prefix} default jenis`}
          value={draft.defaultKind}
          onChange={(e) =>
            update({ defaultKind: e.target.value as DefaultKind })
          }
        >
          <option value="none">Tidak ada</option>
          <option value="text">Teks</option>
          <option value="signed">Bilangan</option>
          <option value="boolean">Boolean</option>
          <option value="null">NULL</option>
        </select>
      </label>
      {draft.defaultKind !== 'none' && draft.defaultKind !== 'null' && (
        <label>
          Nilai default
          <input
            aria-label={`${prefix} default nilai`}
            value={draft.defaultValue}
            onChange={(e) => update({ defaultValue: e.target.value })}
          />
        </label>
      )}
    </fieldset>
  );
  const createFields = type === 'create_table' && (
    <>
      <label>
        Nama tabel
        <input
          value={tableName}
          onChange={(e) => setTableName(e.target.value)}
        />
      </label>
      <label>
        Schema/database namespace
        <input
          disabled={createEngine === 'MySql'}
          value={createEngine === 'MySql' ? '' : schema}
          onChange={(e) => setSchema(e.target.value)}
        />
      </label>
      {columns.map((item, index) => (
        <div key={index}>
          {editor(
            item,
            (patch) =>
              setColumns((current) =>
                current.map((value, i) =>
                  i === index ? { ...value, ...patch } : value,
                ),
              ),
            `Kolom ${index + 1}`,
          )}
          {columns.length > 1 && (
            <button
              type="button"
              onClick={() =>
                setColumns((current) => current.filter((_, i) => i !== index))
              }
            >
              Hapus kolom
            </button>
          )}
        </div>
      ))}
      <button
        type="button"
        onClick={() => setColumns((current) => [...current, emptyColumn()])}
      >
        Tambah kolom
      </button>
    </>
  );
  const columnFields =
    type === 'add_column'
      ? editor(
          column,
          (patch) => setColumn((value) => ({ ...value, ...patch })),
          'Kolom baru',
        )
      : type === 'alter_column' && (
          <>
            <p className="muted small">
              Alter Column mengganti definisi kolom. Nilai default yang sudah
              ada tidak dimuat; masukkan ulang untuk mempertahankannya. “Tidak
              ada” akan menghapus default.
            </p>
            <label>
              Nama kolom
              <input value={name} onChange={(e) => setName(e.target.value)} />
            </label>
            {editor(
              column,
              (patch) => setColumn((value) => ({ ...value, ...patch })),
              'Definisi kolom',
            )}
          </>
        );
  const renameFields = (type === 'rename_table' ||
    type === 'rename_column') && (
    <>
      <>
        {type === 'rename_column' && (
          <label>
            Nama kolom
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        )}
      </>
      <label>
        Nama baru
        <input value={newName} onChange={(e) => setNewName(e.target.value)} />
      </label>
    </>
  );
  const dropFields = (type === 'drop_column' || type === 'drop_index') && (
    <label>
      Nama objek
      <input value={name} onChange={(e) => setName(e.target.value)} />
    </label>
  );
  const indexFields = type === 'create_index' && (
    <>
      <label>
        Nama indeks
        <input
          value={indexName}
          onChange={(e) => setIndexName(e.target.value)}
        />
      </label>
      <label>
        Kolom (pisahkan dengan koma)
        <input
          value={indexColumns}
          onChange={(e) => setIndexColumns(e.target.value)}
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={unique}
          onChange={(e) => setUnique(e.target.checked)}
        />{' '}
        Unique
      </label>
    </>
  );
  const constraintFields = (type === 'add_constraint' ||
    type === 'drop_constraint') && (
    <>
      <label>
        Jenis constraint
        <select
          value={constraintKind}
          onChange={(e) => setConstraintKind(e.target.value as ConstraintKind)}
        >
          <option value="primary_key">Primary key</option>
          <option value="unique">Unique</option>
          <option value="foreign_key">Foreign key</option>
          <option value="check">Check</option>
        </select>
      </label>
      {type === 'add_constraint' && (
        <label>
          Nama constraint
          <input
            value={constraintName}
            onChange={(e) => setConstraintName(e.target.value)}
          />
        </label>
      )}
      {type === 'drop_constraint' && (
        <label>
          Nama constraint
          <input
            value={
              constraintKind === 'primary_key' &&
              requestTable.engine === 'MySql'
                ? 'PRIMARY'
                : name
            }
            disabled={
              constraintKind === 'primary_key' &&
              requestTable.engine === 'MySql'
            }
            onChange={(e) => setName(e.target.value)}
          />
        </label>
      )}
      {type === 'add_constraint' && constraintKind !== 'check' && (
        <label>
          Kolom lokal (pisahkan dengan koma)
          <input
            value={constraintColumns}
            onChange={(e) => setConstraintColumns(e.target.value)}
          />
        </label>
      )}
      {type === 'add_constraint' && constraintKind === 'foreign_key' && (
        <>
          <label>
            Schema tabel referensi
            <input
              value={referencedSchema}
              onChange={(e) => setReferencedSchema(e.target.value)}
            />
          </label>
          <label>
            Tabel referensi
            <input
              value={referencedTable}
              onChange={(e) => setReferencedTable(e.target.value)}
            />
          </label>
          <label>
            Kolom referensi (pisahkan dengan koma)
            <input
              value={referencedColumns}
              onChange={(e) => setReferencedColumns(e.target.value)}
            />
          </label>
        </>
      )}
      {type === 'add_constraint' && constraintKind === 'check' && (
        <label>
          Ekspresi CHECK
          <input
            value={checkExpression}
            onChange={(e) => setCheckExpression(e.target.value)}
          />
        </label>
      )}
    </>
  );
  return (
    <section className="schema-manager" aria-label="Schema manager">
      <h2>Kelola struktur tabel</h2>
      <fieldset disabled={busy || outcomeUnknown} className="schema-controls">
        <legend>Operasi schema</legend>
        <label>
          Operasi
          <select
            aria-label="Operasi"
            value={type}
            onChange={(e) => setType(e.target.value as OperationType)}
          >
            {allowedTypes.map((value) => (
              <option key={value} value={value}>
                {labels[value]}
              </option>
            ))}
          </select>
        </label>
        {createFields}
        {renameFields}
        {columnFields}
        {dropFields}
        {indexFields}
        {constraintFields}
        <div className="schema-actions">
          <button onClick={() => void previewSql()}>Preview SQL</button>
          <button
            className="primary"
            disabled={
              outcomeUnknown ||
              !preview ||
              (preview.destructive &&
                (!confirmed || typedName !== preview.affected_name))
            }
            onClick={() => void apply()}
          >
            Apply
          </button>
        </div>
        {preview ? (
          <pre className="schema-preview" aria-label="SQL preview">
            {preview.sql}
          </pre>
        ) : null}
        {preview?.destructive ? (
          <div className="schema-confirm">
            <strong>Operasi ini dapat menghapus data atau struktur.</strong>
            <label>
              Ketik tepat: {preview.affected_name}
              <input
                aria-label={`Ketik tepat: ${preview.affected_name}`}
                value={typedName}
                onChange={(e) => setTypedName(e.target.value)}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(e) => setConfirmed(e.target.checked)}
              />{' '}
              Saya memahami dampaknya
            </label>
          </div>
        ) : null}
      </fieldset>
      {error ? (
        <p className="error-text" role="alert">
          {error}
        </p>
      ) : null}
      {outcomeUnknown ? (
        <div className="error-text" role="status">
          <p>Outcome Unknown — jangan ulangi Apply otomatis.</p>
          <p>
            Periksa target dan pastikan operasi DDL sudah berhenti sebelum
            melanjutkan.
          </p>
          <button onClick={acknowledgeUnknown}>
            Saya sudah memeriksa database
          </button>
        </div>
      ) : null}
      <p className="muted small">
        Preview tidak menjalankan SQL. Apply dikelola host melalui session DDL
        khusus.
      </p>
    </section>
  );
}
