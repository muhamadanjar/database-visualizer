import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { DatabaseTransport } from '@database-studio/contracts';
import { TableEditor } from './TableEditor';
import { useEditDrafts } from './editDrafts';

const object = {
  engine: 'Postgres' as const,
  database: 'fixture',
  schema: 'public',
  name: 'customers',
  kind: 'Table' as const,
};

it('keeps the existing draft, Preview, and Apply controls in table edit mode', async () => {
  useEditDrafts.setState({ drafts: {} });
  const snapshot = {
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
  const request = vi.fn(async (operation: string) => {
    if (operation === 'edit_snapshot') return snapshot;
    if (operation === 'edit_preview')
      return { statements: ['INSERT INTO customers DEFAULT VALUES'], warnings: [] };
    if (operation === 'edit_apply') return { id: 'mutation-1', state: 'Applied' };
    if (operation === 'edit_readback') return [];
    if (operation === 'edit_discard') return null;
    return null;
  });
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  const transport = { request } as unknown as DatabaseTransport;
  render(
    <TableEditor
      transport={transport}
      connection="connection-1"
      object={object}
      tabId="table-tab"
      onExit={() => {}}
    />,
  );

  fireEvent.click(await screen.findByRole('button', { name: 'Tambah baris' }));
  fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
  await waitFor(() => expect(request).toHaveBeenCalledWith('edit_preview', expect.anything()));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() => expect(request).toHaveBeenCalledWith('edit_apply', expect.anything()));
  expect(screen.getByText(/Applied/)).toBeInTheDocument();
  vi.restoreAllMocks();
});
