import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SchemaManager } from './SchemaManager';
import type { DatabaseTransport, ObjectId, SchemaOperation } from '@database-studio/contracts';

const table = { engine: 'Postgres' as const, database: 'db', schema: 'public', name: 'items', kind: 'Table' as const };
function fakeTransport(handler: DatabaseTransport['request']): DatabaseTransport { return { request: handler, watch: async () => () => {} }; }

it('builds a create-table payload from the create-only context', async () => {
  let request: any;
  const transport = fakeTransport(async (operation, args) => { request = { operation, args }; return { sql: 'CREATE TABLE', destructive: false, affected_name: null }; });
  render(<SchemaManager transport={transport} connectionId="connection" createContext={{ engine: 'MySql', database: 'app', schema: null }} />);
  expect(screen.getByLabelText('Operasi').querySelectorAll('option')).toHaveLength(1);
  expect(screen.getByLabelText('Operasi')).toHaveValue('create_table');
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'create_table' } });
  fireEvent.change(screen.getByLabelText('Nama tabel'), { target: { value: 'accounts' } });
  fireEvent.change(screen.getByLabelText('Schema/database namespace'), { target: { value: '' } });
  fireEvent.change(screen.getByLabelText('Kolom 1 nama'), { target: { value: 'id' } });
  fireEvent.change(screen.getByLabelText('Kolom 1 tipe'), { target: { value: 'BIGINT' } });
  fireEvent.change(screen.getByLabelText('Kolom 1 default jenis'), { target: { value: 'signed' } });
  fireEvent.change(screen.getByLabelText('Kolom 1 default nilai'), { target: { value: '900719925474099312345' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(screen.getByLabelText('SQL preview')).toBeInTheDocument());
  expect(request.args.request).toMatchObject({ table: { engine: 'MySql', database: 'app', schema: null, name: 'accounts' }, operation: { type: 'create_table', columns: [{ name: 'id', data_type: 'BIGINT', default: { type: 'decimal', value: '900719925474099312345' } }] } });
});

it('builds composite unique index and foreign-key fields without placeholders', async () => {
  const requests: any[] = [];
  const transport = fakeTransport(async (operation, args) => { requests.push({ operation, args }); return { sql: 'SQL', destructive: false, affected_name: null }; });
  render(<SchemaManager transport={transport} connectionId="connection" table={table} />);
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'create_index' } });
  fireEvent.change(screen.getByLabelText('Nama indeks'), { target: { value: 'idx_items' } });
  fireEvent.change(screen.getByLabelText('Kolom (pisahkan dengan koma)'), { target: { value: 'tenant_id, email' } });
  fireEvent.click(screen.getByLabelText('Unique'));
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(requests).toHaveLength(1));
  expect(requests[0].args.request.operation).toEqual({ type: 'create_index', index: { name: 'idx_items', columns: ['tenant_id', 'email'], unique: true } });
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'add_constraint' } });
  fireEvent.change(screen.getByLabelText('Jenis constraint'), { target: { value: 'foreign_key' } });
  fireEvent.change(screen.getByLabelText('Nama constraint'), { target: { value: 'fk_items_tenant' } });
  fireEvent.change(screen.getByLabelText('Kolom lokal (pisahkan dengan koma)'), { target: { value: 'tenant_id' } });
  fireEvent.change(screen.getByLabelText('Tabel referensi'), { target: { value: 'tenants' } });
  fireEvent.change(screen.getByLabelText('Kolom referensi (pisahkan dengan koma)'), { target: { value: 'id' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(requests).toHaveLength(2));
  expect(requests[1].args.request.operation.constraint).toMatchObject({ kind: 'foreign_key', columns: ['tenant_id'], referenced_table: { name: 'tenants' }, referenced_columns: ['id'] });
});

it('invalidates preview when inputs change and requires exact destructive confirmation', async () => {
  const calls: string[] = [];
  const payloads: any[] = [];
  const transport = fakeTransport(async (operation, args) => { calls.push(operation); payloads.push(args); if (operation === 'schema_apply') return { execution_id: 'exec' }; if (operation === 'execution') return { status: 'Completed' }; return { sql: 'DROP TABLE "items";', destructive: true, affected_name: 'items' }; });
  render(<SchemaManager transport={transport} connectionId="connection" table={table} />);
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'drop_table' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(screen.getByLabelText('SQL preview')).toBeInTheDocument());
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'drop_column' } });
  expect(screen.queryByLabelText('SQL preview')).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Nama objek'), { target: { value: 'id' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(calls).toEqual(['schema_preview', 'schema_preview']));
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  fireEvent.change(screen.getByLabelText('Ketik tepat: items'), { target: { value: 'items' } });
  fireEvent.click(screen.getByRole('checkbox'));
  expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() => expect(calls).toEqual(['schema_preview', 'schema_preview', 'schema_apply']));
  expect(payloads[2]).toMatchObject({ connection_id: 'connection', request: { confirmation: { confirmed: true, typed_name: 'items' } } });
});

it('locks all operation inputs while preview is in flight', async () => {
  let resolvePreview: (value: unknown) => void = () => {};
  const transport = fakeTransport(async () => new Promise((resolve) => { resolvePreview = resolve; }));
  render(<SchemaManager transport={transport} connectionId="connection" table={table} />);
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => {
    expect(screen.getByLabelText('Operasi')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Preview SQL' })).toBeDisabled();
  });
  resolvePreview({ sql: 'ALTER TABLE', destructive: false, affected_name: null });
  await waitFor(() => expect(screen.getByLabelText('SQL preview')).toBeInTheDocument());
});

async function prepareDrop(onApplied: (operation: SchemaOperation, target?: ObjectId) => void, request: DatabaseTransport['request']) {
  render(<SchemaManager transport={fakeTransport(request)} connectionId="connection" table={table} onApplied={onApplied} />);
  fireEvent.change(screen.getByLabelText('Operasi'), { target: { value: 'drop_table' } });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(screen.getByLabelText('SQL preview')).toBeInTheDocument());
  fireEvent.change(screen.getByLabelText('Ketik tepat: items'), { target: { value: 'items' } });
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
}

it('invokes onApplied only after a completed execution and closes it', async () => {
  const calls: string[] = [];
  let applied = 0;
  let appliedOperation: unknown;
  let appliedTarget: unknown;
  await prepareDrop((operation, target) => { applied += 1; appliedOperation = operation; appliedTarget = target; }, async (operation) => { calls.push(operation); if (operation === 'schema_apply') return { execution_id: 'success-exec' }; if (operation === 'execution') return { status: 'Completed' }; return { sql: 'DROP TABLE "items";', destructive: true, affected_name: 'items' }; });
  await waitFor(() => expect(applied).toBe(1));
  expect(calls).toEqual(['schema_preview', 'schema_apply', 'execution', 'close_execution']);
  expect(appliedOperation).toEqual({ type: 'drop_table' });
  expect(appliedTarget).toEqual(table);
});

it('reports failed and unknown outcomes without refresh or automatic retry', async () => {
  let applied = 0;
  await prepareDrop(() => { applied += 1; }, async (operation) => { if (operation === 'schema_apply') return { execution_id: 'failed-exec' }; if (operation === 'execution') return { status: 'Failed', error: { message: 'permission denied' } }; return { sql: 'DROP TABLE "items";', destructive: true, affected_name: 'items' }; });
  await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('permission denied'));
  expect(applied).toBe(0);

  document.body.innerHTML = '';
  let unknownApplied = 0;
  await prepareDrop(() => { unknownApplied += 1; }, async (operation) => { if (operation === 'schema_apply') return { execution_id: 'unknown-exec' }; if (operation === 'execution') throw new Error('stream lost'); return { sql: 'DROP TABLE "items";', destructive: true, affected_name: 'items' }; });
  await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown'));
  expect(unknownApplied).toBe(0);
});
