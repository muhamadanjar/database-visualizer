import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import type {
  DatabaseTransport,
  DocumentHeader,
  Profile,
} from '@database-studio/contracts';
import { Workspace } from './Workspace';
import { useWorkspace } from './store';
vi.mock('./SqlEditor', () => ({
  default: ({ tab }: { tab: { sql: string } }) => (
    <textarea aria-label="SQL test editor" value={tab.sql} readOnly />
  ),
  disposeEditor: () => {},
}));
const profile: Profile = {
  id: '00000000-0000-4000-8000-000000000001',
  revision: 1,
  name: 'Unit fixture',
  engine: 'Postgres',
  host: 'localhost',
  port: 5432,
  username: 'fixture',
  database: 'fixture',
  ssl: 'verify_full',
  connect_timeout_ms: 10000,
  query_timeout_ms: null,
  history_enabled: true,
  restore_sql: false,
};
const header: DocumentHeader = {
  id: '00000000-0000-4000-8000-000000000002',
  connection_id: profile.id,
  kind: 'saved',
  name: 'Saved SQL',
  created_at_ms: 1,
};
beforeEach(() => useWorkspace.setState({ tabs: [], activeId: null }));
it('keeps the edit action for an existing connection', async () => {
  const request = vi.fn(async (operation: string) => {
    if (operation === 'handshake') return { version: 1 };
    if (operation === 'profiles') return [profile];
    if (operation === 'layout') return {};
    if (operation === 'recent_files') return [];
    return null;
  });
  const transport = {
    capabilities: {
      platform: 'desktop' as const,
      manageConnections: true,
      execute: true,
      persistentCredentials: true,
    },
    request,
    watch: async () => () => {},
  } as unknown as DatabaseTransport;
  render(<Workspace transport={transport} />);
  fireEvent.click(
    await screen.findByRole(
      'button',
      { name: /Unit fixture/ },
      { timeout: 5000 },
    ),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Edit' }));
  expect(
    await screen.findByRole('heading', { name: 'Edit koneksi' }),
  ).toBeInTheDocument();
});

it('runs a DML statement through the visible SQL editor Run action', async () => {
  const request = vi.fn(async (operation: string) => {
    if (operation === 'profiles') return [profile];
    if (operation === 'layout') return {};
    if (operation === 'recent_files') return [];
    if (operation === 'metadata')
      return {
        objects: [],
        columns: [],
        indexes: [],
        constraints: [],
        ddl: null,
      };
    if (operation === 'connect')
      return {
        id: '00000000-0000-4000-8000-000000000003',
        connection_id: profile.id,
        revision: 1,
        auto_commit: true,
        transaction: 'Idle',
        valid: true,
        busy: false,
      };
    if (operation === 'prepare_script')
      return [
        {
          sql: "UPDATE items SET name = 'changed' WHERE id = 1;",
          start: 0,
          end: 49,
        },
      ];
    if (operation === 'execute') return { execution_id: 'execution-dml' };
    if (operation === 'execution')
      return {
        id: 'execution-dml',
        session_id: '00000000-0000-4000-8000-000000000003',
        connection_id: profile.id,
        status: 'Completed',
        results: [
          {
            id: 'result-dml',
            statement: 0,
            columns: [],
            rows: 0,
            terminal: {
              type: 'query_completed',
              summary: {
                results_complete: true,
                affected_rows: '1',
                rows_returned: '0',
              },
              elapsed_ms: 1,
            },
          },
        ],
        error: null,
        started_at_ms: 1,
        finished_at_ms: 2,
        persistence_error: null,
      };
    if (operation === 'session')
      return {
        id: '00000000-0000-4000-8000-000000000003',
        connection_id: profile.id,
        revision: 1,
        auto_commit: true,
        transaction: 'Idle',
        valid: true,
        busy: false,
      };
    return null;
  });
  const transport = {
    request,
    watch: async () => () => {},
  } as unknown as DatabaseTransport;
  render(<Workspace transport={transport} />);
  fireEvent.click(await screen.findByText('Unit fixture'));
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
  await screen.findByText('Connected');
  fireEvent.click(screen.getByRole('button', { name: 'SQL tab' }));
  const tab = useWorkspace.getState().tabs.at(-1)!;
  useWorkspace.getState().patch(tab.id, {
    sql: "UPDATE items SET name = 'changed' WHERE id = 1;",
  });
  await waitFor(() =>
    expect(screen.getByLabelText('SQL test editor')).toHaveValue(
      "UPDATE items SET name = 'changed' WHERE id = 1;",
    ),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Run' }));
  await waitFor(() =>
    expect(request).toHaveBeenCalledWith('execute', {
      session_id: '00000000-0000-4000-8000-000000000003',
      sql: "UPDATE items SET name = 'changed' WHERE id = 1;",
    }),
  );
  expect(
    await screen.findByText('Affected rows: 1, returned rows: 0'),
  ).toBeTruthy();
});

it('shows the database error when a later DML statement fails', async () => {
  const failed = {
    id: 'execution-failed',
    session_id: 'session-failed',
    connection_id: profile.id,
    status: 'Failed' as const,
    results: [
      {
        id: 'result-success',
        statement: 0,
        columns: [],
        rows: 0,
        terminal: {
          type: 'query_completed',
          summary: {
            results_complete: true,
            affected_rows: '1',
            rows_returned: '0',
          },
          elapsed_ms: 1,
        },
      },
      {
        id: 'result-failure',
        statement: 1,
        columns: [],
        rows: 0,
        terminal: {
          type: 'query_failed',
          error: {
            code: 'CONSTRAINT_VIOLATION',
            category: 'database',
            message: 'Unique constraint violated',
          },
          elapsed_ms: 1,
        },
      },
    ],
    error: null,
    started_at_ms: 1,
    finished_at_ms: 2,
    persistence_error: null,
  };
  useWorkspace.getState().add({
    id: 'failed-tab',
    connectionId: profile.id,
    title: 'Write SQL',
    sql: 'UPDATE items SET id = 1',
    execution: failed,
  });
  const request = vi.fn(async (operation: string) => {
    if (operation === 'profiles') return [profile];
    if (operation === 'layout') return {};
    if (operation === 'recent_files') return [];
    return null;
  });
  render(
    <Workspace
      transport={{ request, watch: async () => () => {} } as DatabaseTransport}
    />,
  );
  expect(await screen.findByText('Unique constraint violated')).toBeTruthy();
});

it('refreshes inspector metadata and table data to observable new values', async () => {
  const object = {
    engine: 'Postgres' as const,
    database: 'fixture',
    schema: 'public',
    name: 'items',
    kind: 'Table' as const,
  };
  let metadataVersion = 0;
  let dataVersion = 0;
  const session = {
    id: '00000000-0000-4000-8000-000000000004',
    connection_id: profile.id,
    revision: 1,
    auto_commit: true,
    transaction: 'Idle' as const,
    valid: true,
    busy: false,
  };
  const request = vi.fn(
    async (operation: string, args?: Record<string, unknown>) => {
      if (operation === 'profiles') return [profile];
      if (operation === 'layout') return {};
      if (operation === 'recent_files') return [];
      if (operation === 'connect') return session;
      if (operation === 'metadata') {
        const requestBody = args?.request;
        if (requestBody === 'Schemas')
          return {
            objects: [{ ...object, name: 'public', kind: 'Schema' }],
            columns: [],
            indexes: [],
            constraints: [],
            ddl: null,
          };
        if (
          requestBody &&
          typeof requestBody === 'object' &&
          'Objects' in requestBody
        )
          return {
            objects: [object],
            columns: [],
            indexes: [],
            constraints: [],
            ddl: null,
          };
        metadataVersion += 1;
        return {
          objects: [],
          columns: [
            {
              name: metadataVersion === 1 ? 'old_name' : 'new_name',
              data_type: 'text',
              nullable: true,
              default: null,
              ordinal: 1,
            },
          ],
          indexes: [],
          constraints: [],
          ddl: null,
          editing: null,
        };
      }
      if (operation === 'table') {
        dataVersion += 1;
        return { execution_id: `execution-data-${dataVersion}` };
      }
      if (operation === 'execution') {
        const id = String(args?.execution_id);
        const version = id.endsWith('2') ? 2 : 1;
        return {
          id,
          session_id: session.id,
          connection_id: profile.id,
          status: 'Completed',
          results: [
            {
              id: `result-${version}`,
              statement: 0,
              columns: [{ name: 'name', database_type: 'text' }],
              rows: 1,
              terminal: {
                type: 'query_completed',
                summary: {
                  results_complete: true,
                  affected_rows: '0',
                  rows_returned: '1',
                },
                elapsed_ms: 1,
              },
            },
          ],
          error: null,
          started_at_ms: 1,
          finished_at_ms: 2,
          persistence_error: null,
        };
      }
      if (operation === 'page')
        return [
          [{ kind: 'text', value: dataVersion === 1 ? 'before' : 'after' }],
        ];
      if (operation === 'close_execution') return null;
      return null;
    },
  );
  const transport = {
    request,
    watch: async () => () => {},
  } as unknown as DatabaseTransport;
  render(<Workspace transport={transport} />);
  fireEvent.click(await screen.findByText('Unit fixture'));
  fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
  await screen.findByText('Connected');
  useWorkspace.getState().add({
    id: 'inspector-tab',
    connectionId: profile.id,
    title: 'items',
    sql: '',
    object,
    inspector: true,
  });
  expect(await screen.findByText('old_name')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Data' }));
  await waitFor(() =>
    expect(
      request.mock.calls.filter(([operation]) => operation === 'table'),
    ).toHaveLength(1),
  );
  fireEvent.click(screen.getByRole('button', { name: /^Refresh$/ }));
  await waitFor(() =>
    expect(
      request.mock.calls.filter(([operation]) => operation === 'table'),
    ).toHaveLength(2),
  );
  fireEvent.click(screen.getByRole('button', { name: 'Columns' }));
  expect(await screen.findByText('new_name')).toBeTruthy();
});

it('loads document headers, pulls one SQL document, and saves with its own ID', async () => {
  const request = vi.fn(async (operation: string) => {
    if (operation === 'profiles') return [profile];
    if (operation === 'documents') return [header];
    if (operation === 'document')
      return { ...header, content: { sql: 'SELECT 9007199254740993;' } };
    if (operation === 'layout') return {};
    if (operation === 'recent_files') return [];
    return null;
  });
  const transport = {
    request,
    watch: async () => () => {},
  } as unknown as DatabaseTransport;
  render(<Workspace transport={transport} />);
  fireEvent.click(await screen.findByText('Unit fixture'));
  fireEvent.click(screen.getByRole('button', { name: 'Saved queries' }));
  fireEvent.click(await screen.findByText('Saved SQL'));
  expect(await screen.findByLabelText('SQL test editor')).toHaveValue(
    'SELECT 9007199254740993;',
  );
  expect(request.mock.calls.filter(([op]) => op === 'document')).toHaveLength(
    1,
  );
  expect(
    request.mock.calls.some(([op]) => op === 'execute' || op === 'connect'),
  ).toBe(false);
  vi.spyOn(window, 'prompt').mockReturnValue('Edited SQL');
  fireEvent.click(screen.getByRole('button', { name: 'Simpan query' }));
  await waitFor(() =>
    expect(request).toHaveBeenCalledWith(
      'save_document',
      expect.objectContaining({
        document: expect.objectContaining({
          id: header.id,
          kind: 'saved',
          name: 'Edited SQL',
        }),
      }),
    ),
  );
  expect(useWorkspace.getState().tabs[0].id).not.toBe(header.id);
  vi.restoreAllMocks();
});

it('enters table edit mode from Data and returns through the draft guard', async () => {
  const object = {
    engine: 'Postgres' as const,
    database: profile.database,
    schema: 'public',
    name: 'customers',
    kind: 'Table' as const,
  };
  const otherProfile = {
    ...profile,
    id: '00000000-0000-4000-8000-000000000003',
    name: 'Second fixture',
  };
  const execution = {
    id: 'execution-1',
    session_id: 'session-1',
    connection_id: profile.id,
    status: 'Completed' as const,
    results: [],
    error: null,
    started_at_ms: 1,
    finished_at_ms: 2,
    persistence_error: null,
  };
  useWorkspace.setState({
    tabs: [
      {
        id: 'table-tab',
        connectionId: profile.id,
        title: object.name,
        sql: '',
        object,
        inspector: true,
        execution,
        session: {
          id: 'session-1',
          connection_id: profile.id,
          revision: 1,
          auto_commit: true,
          transaction: 'Idle',
          valid: true,
          busy: false,
        },
      },
    ],
    activeId: 'table-tab',
  });
  let failMetadata = false;
  const request = vi.fn(
    async (operation: string, args?: Record<string, unknown>) => {
      if (operation === 'profiles') return [profile, otherProfile];
      if (operation === 'metadata') {
        if (failMetadata) throw new Error('metadata refresh failed');
        if (args?.request === 'Schemas')
          return { objects: [{ name: 'public' }] };
        if (
          args?.request &&
          typeof args.request === 'object' &&
          'Objects' in args.request
        )
          return { objects: [object] };
        return {
          objects: [object],
          columns: [
            {
              name: 'id',
              data_type: 'integer',
              nullable: false,
              default: null,
              ordinal: 0,
            },
          ],
          indexes: [],
          constraints: [],
          ddl: null,
          editing: {
            primary_key: ['id'],
            generated: [],
            transactional: true,
            warnings: [],
          },
        };
      }
      if (operation === 'schema_preview')
        return {
          sql: 'DROP TABLE "customers";',
          destructive: true,
          affected_name: 'customers',
          profile_revision: 1,
        };
      if (operation === 'schema_apply') return { execution_id: 'unknown-ddl' };
      if (operation === 'execution') throw new Error('stream lost');
      if (operation === 'layout' || operation === 'recent_files') return [];
      if (operation === 'edit_snapshot')
        return {
          token: 'snapshot-token',
          offset: 0,
          columns: [
            {
              name: 'id',
              data_type: 'integer',
              nullable: false,
              default: null,
              ordinal: 0,
            },
          ],
          rows: [],
          primary_key: ['id'],
          generated: [],
          warnings: [],
        };
      return null;
    },
  );
  const transport = {
    capabilities: {
      platform: 'desktop' as const,
      manageConnections: true,
      execute: true,
      persistentCredentials: true,
    },
    request,
    watch: async () => () => {},
  } as unknown as DatabaseTransport;
  render(<Workspace transport={transport} />);

  fireEvent.click(await screen.findByRole('button', { name: 'Data' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Edit table' }));
  expect(
    await screen.findByRole('button', { name: 'Tambah baris' }),
  ).toBeInTheDocument();

  fireEvent.click(screen.getByRole('button', { name: 'Kembali ke Data' }));
  expect(
    await screen.findByRole('button', { name: 'Edit table' }),
  ).toBeInTheDocument();
  expect(request).toHaveBeenCalledWith(
    'edit_discard',
    expect.objectContaining({ tab_id: 'table-tab' }),
  );
  fireEvent.click(await screen.findByRole('button', { name: /Unit fixture/ }));
  fireEvent.click(await screen.findByRole('button', { name: 'public' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Tables' }));
  fireEvent.click(
    await screen.findByRole('button', { name: /Second fixture/ }),
  );
  fireEvent.click(await screen.findByRole('button', { name: 'Structure' }));
  fireEvent.change(screen.getByLabelText('Operasi'), {
    target: { value: 'drop_table' },
  });
  fireEvent.click(screen.getByRole('button', { name: 'Preview SQL' }));
  await screen.findByLabelText('SQL preview');
  fireEvent.change(screen.getByLabelText('Ketik tepat: customers'), {
    target: { value: 'customers' },
  });
  fireEvent.click(screen.getByRole('checkbox'));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() =>
    expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown'),
  );
  failMetadata = true;
  fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  await waitFor(() =>
    expect(screen.getByText('metadata refresh failed')).toBeInTheDocument(),
  );
  expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown');
  failMetadata = false;
  fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
  await waitFor(() =>
    expect(
      screen.queryByText('metadata refresh failed'),
    ).not.toBeInTheDocument(),
  );
  expect(screen.getByRole('status')).toHaveTextContent('Outcome Unknown');

  expect(screen.getByRole('button', { name: 'SQL tab' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Saved queries' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'History' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Data' })).toBeDisabled();
  expect(screen.getByRole('button', { name: /Second fixture/ })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Connect' })).toBeDisabled();
});
