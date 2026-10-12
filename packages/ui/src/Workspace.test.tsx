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
