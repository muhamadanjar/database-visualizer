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
      {
        name: 'label',
        data_type: 'text',
        nullable: false,
        default: null,
        ordinal: 1,
      },
    ],
    rows: [
      [
        { kind: 'value', value: '1' },
        { kind: 'value', value: 'Original label' },
      ],
    ],
    primary_key: ['id'],
    generated: [],
    warnings: [],
  };
  const request = vi.fn(async (operation: string) => {
    if (operation === 'edit_snapshot') return snapshot;
    if (operation === 'edit_preview')
      return {
        statements: [
          "UPDATE customers SET label = 'Updated label' WHERE id = 1",
        ],
        warnings: [],
      };
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

  fireEvent.change(
    await screen.findByRole('textbox', { name: 'label baris 1' }),
    { target: { value: 'Updated label' } },
  );
  fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
  const expectedChange = {
    type: 'update' as const,
    row: 0,
    values: {
      label: { type: 'value' as const, value: 'Updated label' },
    },
  };
  await waitFor(() => {
    const call = request.mock.calls.find(([operation]) => operation === 'edit_preview');
    expect(call?.[1]).toEqual({
      connection_id: 'connection-1',
      tab_id: 'table-tab',
      batch: {
        tab_id: 'table-tab',
        token: 'snapshot-token',
        changes: [expectedChange],
      },
    });
  });
  const previewCall = request.mock.calls.find(
    ([operation]) => operation === 'edit_preview',
  );
  const previewBatch = (previewCall?.[1] as { batch: unknown }).batch;
  expect(
    await screen.findByText(
      "UPDATE customers SET label = 'Updated label' WHERE id = 1",
    ),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() => {
    const call = request.mock.calls.find(([operation]) => operation === 'edit_apply');
    expect(call?.[1]).toEqual(
      expect.objectContaining({
        batch: previewBatch,
      }),
    );
  });
  expect(screen.getByText(/Applied/)).toBeInTheDocument();
  vi.restoreAllMocks();
});
