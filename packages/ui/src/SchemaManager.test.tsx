import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { useState } from 'react';
import '@testing-library/jest-dom/vitest';
import { SchemaManager } from './SchemaManager';
import type {
  DatabaseTransport,
  ObjectId,
  SchemaOperation,
} from '@database-studio/contracts';

const table = {
  engine: 'Postgres' as const,
  database: 'db',
  schema: 'public',
  name: 'items',
  kind: 'Table' as const,
};
type RequestHandler = (
  operation: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;
function fakeTransport(handler: RequestHandler): DatabaseTransport {
  return {
    request: async <T,>(operation: string, args?: Record<string, unknown>) =>
      (await handler(operation, args)) as T,
    watch: async () => () => {},
  };
}

it('builds a create-table payload from the create-only context', async () => {
  let request: any;
  const transport = fakeTransport(async (operation, args) => {
    request = { operation, args };
    return {
      sql: 'CREATE TABLE',
      destructive: false,
      affected_name: null,
      profile_revision: 1,
    };
  });
  render(
    <SchemaManager
      transport={transport}
      connectionId="connection"
      connectionRevision={1}
      createContext={{ engine: 'MySql', database: 'app', schema: null }}
    />,
  );
  expect(
    screen.getByLabelText('Operasi').querySelectorAll('option'),
  ).toHaveLength(1);
  expect(screen.getByLabelText('Operasi')).toHaveValue('create_table');
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'create_table' },
  });
  fireEvent.change(screen.getByLabelText('Nama tabel'), {
    target: { value: 'accounts' },
  });
  fireEvent.change(screen.getByLabelText('Schema/database namespace'), {
    target: { value: '' },
  });
  fireEvent.change(screen.getByLabelText('Kolom 1 nama'), {
    target: { value: 'id' },
  });
  fireEvent.change(screen.getByLabelText('Kolom 1 tipe'), {
    target: { value: 'BIGINT' },
  });
  fireEvent.change(screen.getByLabelText('Kolom 1 default jenis'), {
    target: { value: 'signed' },
  });
  fireEvent.change(screen.getByLabelText('Kolom 1 default nilai'), {
    target: { value: '900719925474099312345' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() =>
    expect(screen.getByLabelText('SQL preview')).toBeInTheDocument(),
  );
  expect(request.args.request).toMatchObject({
    table: { engine: 'MySql', database: 'app', schema: null, name: 'accounts' },
    operation: {
      type: 'create_table',
      columns: [
        {
          name: 'id',
          data_type: 'BIGINT',
          default: { type: 'decimal', value: '900719925474099312345' },
        },
      ],
    },
  });
});

it('builds composite unique index and foreign-key fields without placeholders', async () => {
  const requests: any[] = [];
  const transport = fakeTransport(async (operation, args) => {
    requests.push({ operation, args });
    return {
      sql: 'SQL',
      destructive: false,
      affected_name: null,
      profile_revision: 1,
    };
  });
  render(
    <SchemaManager
      transport={transport}
      connectionId="connection"
      connectionRevision={1}
      table={table}
    />,
  );
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'create_index' },
  });
  fireEvent.change(screen.getByLabelText('Nama indeks'), {
    target: { value: 'idx_items' },
  });
  fireEvent.change(screen.getByLabelText('Kolom (pisahkan dengan koma)'), {
    target: { value: 'tenant_id, email' },
  });
  fireEvent.click(screen.getByLabelText('Unique'));
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(requests).toHaveLength(1));
  expect(requests[0].args.request.operation).toEqual({
    type: 'create_index',
    index: { name: 'idx_items', columns: ['tenant_id', 'email'], unique: true },
  });
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'add_constraint' },
  });
  fireEvent.change(screen.getByLabelText('Jenis constraint'), {
    target: { value: 'foreign_key' },
  });
  fireEvent.change(screen.getByLabelText('Nama constraint'), {
    target: { value: 'fk_items_tenant' },
  });
  fireEvent.change(
    screen.getByLabelText('Kolom lokal (pisahkan dengan koma)'),
    { target: { value: 'tenant_id' } },
  );
  fireEvent.change(screen.getByLabelText('Tabel referensi'), {
    target: { value: 'tenants' },
  });
  fireEvent.change(
    screen.getByLabelText('Kolom referensi (pisahkan dengan koma)'),
    { target: { value: 'id' } },
  );
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => expect(requests).toHaveLength(2));
  expect(requests[1].args.request.operation.constraint).toMatchObject({
    kind: 'foreign_key',
    columns: ['tenant_id'],
    referenced_table: { name: 'tenants' },
    referenced_columns: ['id'],
  });
});

it('invalidates preview when inputs change and requires exact destructive confirmation', async () => {
  const calls: string[] = [];
  const payloads: any[] = [];
  const transport = fakeTransport(async (operation, args) => {
    calls.push(operation);
    payloads.push(args);
    if (operation === 'schema_apply') return { execution_id: 'exec' };
    if (operation === 'execution') return { status: 'Completed' };
    return {
      sql: 'DROP TABLE "items";',
      destructive: true,
      affected_name: 'items',
      profile_revision: 1,
    };
  });
  render(
    <SchemaManager
      transport={transport}
      connectionId="connection"
      connectionRevision={1}
      table={table}
    />,
  );
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'drop_table' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() =>
    expect(screen.getByLabelText('SQL preview')).toBeInTheDocument(),
  );
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'drop_column' },
  });
  expect(screen.queryByLabelText('SQL preview')).not.toBeInTheDocument();
  fireEvent.change(screen.getByLabelText('Nama objek'), {
    target: { value: 'id' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() =>
    expect(calls).toEqual(['schema_preview', 'schema_preview']),
  );
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  fireEvent.change(screen.getByLabelText('Ketik tepat: items'), {
    target: { value: 'items' },
  });
  fireEvent.click(screen.getByRole('checkbox'));
  expect(screen.getByRole('button', { name: 'Apply' })).toBeEnabled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() =>
    expect(calls).toEqual(['schema_preview', 'schema_preview', 'schema_apply']),
  );
  expect(payloads[2]).toMatchObject({
    connection_id: 'connection',
    expected_profile_revision: 1,
    request: { confirmation: { confirmed: true, typed_name: 'items' } },
  });
});

it('locks all operation inputs while preview is in flight', async () => {
  let resolvePreview: (value: unknown) => void = () => {};
  const transport = fakeTransport(
    async () =>
      new Promise<unknown>((resolve) => {
        resolvePreview = resolve;
      }),
  );
  render(
    <SchemaManager
      transport={transport}
      connectionId="connection"
      connectionRevision={1}
      table={table}
    />,
  );
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() => {
    expect(screen.getByLabelText('Operasi')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Preview SQL' })).toBeDisabled();
  });
  resolvePreview({
    sql: 'ALTER TABLE',
    destructive: false,
    affected_name: null,
    profile_revision: 1,
  });
  await waitFor(() =>
    expect(screen.getByLabelText('SQL preview')).toBeInTheDocument(),
  );
});

async function prepareDrop(
  onApplied: (operation: SchemaOperation, target?: ObjectId) => void,
  request: RequestHandler,
  onBusyChange?: (busy: boolean) => void,
) {
  render(
    <SchemaManager
      transport={fakeTransport(request)}
      connectionId="connection"
      connectionRevision={1}
      table={table}
      onApplied={onApplied}
      onBusyChange={onBusyChange}
    />,
  );
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'drop_table' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await waitFor(() =>
    expect(screen.getByLabelText('SQL preview')).toBeInTheDocument(),
  );
  fireEvent.change(screen.getByLabelText('Ketik tepat: items'), {
    target: { value: 'items' },
  });
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
}

it('invokes onApplied only after a completed execution and closes it', async () => {
  const calls: string[] = [];
  let applied = 0;
  let appliedOperation: unknown;
  let appliedTarget: unknown;
  await prepareDrop(
    (operation, target) => {
      applied += 1;
      appliedOperation = operation;
      appliedTarget = target;
    },
    async (operation) => {
      calls.push(operation);
      if (operation === 'schema_apply') return { execution_id: 'success-exec' };
      if (operation === 'execution')
        return { status: 'Completed', results: [] };
      return {
        sql: 'DROP TABLE "items";',
        destructive: true,
        profile_revision: 1,
        affected_name: 'items',
      };
    },
  );
  await waitFor(() => expect(applied).toBe(1));
  expect(calls).toEqual([
    'schema_preview',
    'schema_apply',
    'execution',
    'close_execution',
  ]);
  expect(appliedOperation).toEqual({ type: 'drop_table' });
  expect(appliedTarget).toEqual(table);
});
it('does not re-notify schema busy state when the parent callback identity changes', async () => {
  cleanup();
  const busyStates: boolean[] = [];
  let applied = false;
  const transport = fakeTransport(async (operation) => {
    if (operation === 'schema_preview')
      return {
        sql: 'DROP TABLE "items";',
        destructive: true,
        profile_revision: 1,
        affected_name: 'items',
      };
    if (operation === 'schema_apply') return { execution_id: 'success-exec' };
    if (operation === 'execution') return { status: 'Completed', results: [] };
    return null;
  });
  function RerenderingManager() {
    const [, rerender] = useState(0);
    return (
      <SchemaManager
        transport={transport}
        connectionId="connection"
        connectionRevision={1}
        table={table}
        onApplied={() => {
          applied = true;
        }}
        onBusyChange={(busy) => {
          busyStates.push(busy);
          rerender((version) => version + 1);
        }}
      />
    );
  }
  render(<RerenderingManager />);
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'drop_table' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await screen.findByLabelText('SQL preview');
  fireEvent.change(screen.getByLabelText('Ketik tepat: items'), {
    target: { value: 'items' },
  });
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() => expect(applied).toBe(true));
  await waitFor(() =>
    expect(busyStates).toEqual([false, true, false, true, false]),
  );
});

it('locks navigation on unknown polling and requires an explicit inspection acknowledgment', async () => {
  cleanup();
  const calls: string[] = [];
  const lockStates: boolean[] = [];
  let applied = 0;
  await prepareDrop(
    () => {
      applied += 1;
    },
    async (operation) => {
      calls.push(operation);
      if (operation === 'schema_apply') return { execution_id: 'unknown-exec' };
      if (operation === 'execution') throw new Error('stream lost');
      return {
        sql: 'DROP TABLE "items";',
        destructive: true,
        affected_name: 'items',
        profile_revision: 1,
      };
    },
    (locked) => lockStates.push(locked),
  );
  await waitFor(() =>
    expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown'),
  );
  expect(applied).toBe(0);
  expect(lockStates).toContain(true);
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  expect(screen.getByLabelText('Operasi')).toBeDisabled();

  vi.spyOn(window, 'confirm').mockReturnValue(true);
  fireEvent.click(
    screen.getByRole('button', { name: 'Saya sudah memeriksa database' }),
  );
  await waitFor(() => expect(screen.getByLabelText('Operasi')).toBeEnabled());
  expect(screen.queryByLabelText('SQL preview')).not.toBeInTheDocument();
  await waitFor(() => expect(calls).toContain('close_execution'));
  expect(lockStates.at(-1)).toBe(false);
  vi.restoreAllMocks();
});

it('does not close a terminal execution whose DDL outcome is unknown', async () => {
  cleanup();
  const calls: string[] = [];
  await prepareDrop(
    () => {},
    async (operation) => {
      calls.push(operation);
      if (operation === 'schema_apply')
        return { execution_id: 'unknown-terminal' };
      if (operation === 'execution')
        return {
          status: 'Failed',
          error: { message: 'connection lost' },
          results: [{ terminal: { outcome: 'Unknown' } }],
        };
      return {
        sql: 'DROP TABLE "items";',
        destructive: true,
        affected_name: 'items',
        profile_revision: 1,
      };
    },
  );
  await waitFor(() =>
    expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown'),
  );
  expect(calls).not.toContain('close_execution');
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
});

it('treats a lost Apply admission response as unknown instead of retryable', async () => {
  cleanup();
  await prepareDrop(
    () => {},
    async (operation) => {
      if (operation === 'schema_apply') throw new Error('response lost');
      return {
        sql: 'DROP TABLE "items";',
        destructive: true,
        affected_name: 'items',
        profile_revision: 1,
      };
    },
  );
  await waitFor(() =>
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Respons Apply DDL tidak diterima dengan pasti',
    ),
  );
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  expect(screen.getByLabelText('Operasi')).toBeDisabled();
});
