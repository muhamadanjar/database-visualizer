import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import type { DatabaseTransport } from '@database-studio/contracts';
import { TableEditor } from './TableEditor';
import { useEditDrafts, type EditSnapshot } from './editDrafts';

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
    profile_revision: 1,
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
  const request = vi.fn(
    async (operation: string, _args?: Record<string, unknown>) => {
      if (operation === 'edit_snapshot') return snapshot;
      if (operation === 'edit_preview')
        return {
          statements: [
            "UPDATE customers SET label = 'Updated label' WHERE id = 1",
          ],
          warnings: [],
        };
      if (operation === 'edit_apply')
        return { id: 'mutation-1', state: 'Applied' };
      if (operation === 'edit_readback') return [];
      if (operation === 'edit_discard') return null;
      return null;
    },
  );
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
    const call = request.mock.calls.find(
      ([operation]) => operation === 'edit_preview',
    );
    expect(call?.[1]).toEqual({
      connection_id: 'connection-1',
      expected_profile_revision: 1,
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
  if (!previewCall) throw new Error('Expected preview request');
  const previewArgs = previewCall[1];
  if (
    !previewArgs ||
    typeof previewArgs !== 'object' ||
    !('batch' in previewArgs)
  )
    throw new Error('Expected preview batch');
  const previewBatch = previewArgs.batch;
  expect(
    await screen.findByText(
      "UPDATE customers SET label = 'Updated label' WHERE id = 1",
    ),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() => {
    const call = request.mock.calls.find(
      ([operation]) => operation === 'edit_apply',
    );
    expect(call?.[1]).toEqual(
      expect.objectContaining({
        expected_profile_revision: 1,
        batch: previewBatch,
      }),
    );
  });
  expect(screen.getByText(/Applied/)).toBeInTheDocument();
  vi.restoreAllMocks();
});
it('locks edits during Preview and ignores a stale response after state changes', async () => {
  useEditDrafts.setState({ drafts: {} });
  let resolvePreview: (value: {
    statements: string[];
    warnings: string[];
  }) => void = () => {};
  const pendingPreview = new Promise<{
    statements: string[];
    warnings: string[];
  }>((resolve) => {
    resolvePreview = resolve;
  });
  const snapshot = {
    token: 'snapshot-token',
    profile_revision: 1,
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
    if (operation === 'edit_preview') return pendingPreview;
    return null;
  });
  render(
    <TableEditor
      transport={{ request } as unknown as DatabaseTransport}
      connection="connection-1"
      object={object}
      tabId="table-tab"
      onExit={() => {}}
    />,
  );
  fireEvent.change(
    await screen.findByRole('textbox', { name: 'label baris 1' }),
    { target: { value: 'Draft label' } },
  );
  fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
  await waitFor(() =>
    expect(request).toHaveBeenCalledWith('edit_preview', expect.anything()),
  );
  expect(screen.getByRole('textbox', { name: 'label baris 1' })).toBeDisabled();
  const current = useEditDrafts.getState().drafts['table-tab'];
  if (!current) throw new Error('Expected draft before preview response');
  useEditDrafts.getState().put('table-tab', {
    ...current,
    state: 'Applying',
    mutation: 'mutation-pending',
  });
  resolvePreview({ statements: ['stale preview'], warnings: [] });
  await waitFor(() =>
    expect(useEditDrafts.getState().drafts['table-tab']).toMatchObject({
      state: 'Applying',
      mutation: 'mutation-pending',
    }),
  );
  expect(useEditDrafts.getState().drafts['table-tab']?.preview).toBeUndefined();
  useEditDrafts.setState({ drafts: {} });
});

it('locks editing and preserves a draft while refresh discard is pending', async () => {
  useEditDrafts.setState({ drafts: {} });
  let resolveDiscard: () => void = () => {};
  const discard = new Promise<void>((resolve) => {
    resolveDiscard = resolve;
  });
  const snapshot = {
    token: 'snapshot-token',
    profile_revision: 1,
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
    if (operation === 'edit_discard') return discard;
    return null;
  });
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  render(
    <TableEditor
      transport={{ request } as unknown as DatabaseTransport}
      connection="connection-1"
      object={object}
      tabId="table-tab"
      onExit={() => {}}
    />,
  );
  fireEvent.change(
    await screen.findByRole('textbox', { name: 'label baris 1' }),
    {
      target: { value: 'Draft label' },
    },
  );
  fireEvent.click(
    screen.getByRole('button', { name: 'Refresh / buang draft' }),
  );
  await waitFor(() =>
    expect(request).toHaveBeenCalledWith('edit_discard', {
      connection_id: 'connection-1',
      tab_id: 'table-tab',
    }),
  );
  expect(screen.getByRole('textbox', { name: 'label baris 1' })).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();

  const current = useEditDrafts.getState().drafts['table-tab'];
  if (!current) throw new Error('Expected draft while discard is pending');
  useEditDrafts.getState().put('table-tab', {
    ...current,
    state: 'Applying',
    mutation: 'mutation-pending',
  });
  resolveDiscard();
  await waitFor(() =>
    expect(useEditDrafts.getState().drafts['table-tab']).toMatchObject({
      state: 'Applying',
      mutation: 'mutation-pending',
      changes: [
        expect.objectContaining({
          type: 'update',
          values: { label: { type: 'value', value: 'Draft label' } },
        }),
      ],
    }),
  );
  expect(
    request.mock.calls.filter(([operation]) => operation === 'edit_snapshot'),
  ).toHaveLength(1);
  useEditDrafts.setState({ drafts: {} });
  vi.restoreAllMocks();
});

it('treats stale-profile Apply rejection as known failure, not Unknown', async () => {
  useEditDrafts.setState({ drafts: {} });
  const snapshot = {
    token: 'snapshot-token',
    profile_revision: 1,
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
      return { statements: ['UPDATE customers SET label'], warnings: [] };
    if (operation === 'edit_apply')
      throw Object.assign(new Error('Preview expired'), {
        code: 'SCHEMA_PROFILE_CHANGED',
      });
    return null;
  });
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  render(
    <TableEditor
      transport={{ request } as unknown as DatabaseTransport}
      connection="connection-1"
      object={object}
      tabId="table-tab"
      onExit={() => {}}
    />,
  );
  fireEvent.change(
    await screen.findByRole('textbox', { name: 'label baris 1' }),
    {
      target: { value: 'Draft label' },
    },
  );
  fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
  await screen.findByText('UPDATE customers SET label');
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await screen.findByText('Preview expired');
  expect(useEditDrafts.getState().drafts['table-tab']?.state).toBe('Failed');
  expect(screen.queryByText(/Commit tidak dapat dipastikan/)).toBeNull();
  useEditDrafts.setState({ drafts: {} });
  vi.restoreAllMocks();
});

it('keeps edits locked across an editor remount during Apply refresh', async () => {
  useEditDrafts.setState({ drafts: {} });
  let resolveRefresh: (snapshot: EditSnapshot) => void = () => {};
  const refresh = new Promise<EditSnapshot>((resolve) => {
    resolveRefresh = resolve;
  });
  const snapshot = {
    token: 'snapshot-token',
    profile_revision: 1,
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
  const refreshed = {
    ...snapshot,
    rows: [
      [
        { kind: 'value', value: '1' },
        { kind: 'value', value: 'Updated label' },
      ],
    ],
  };
  let snapshots = 0;
  const request = vi.fn(async (operation: string) => {
    if (operation === 'edit_snapshot')
      return ++snapshots === 1 ? snapshot : refresh;
    if (operation === 'edit_preview')
      return { statements: ['UPDATE customers SET label'], warnings: [] };
    if (operation === 'edit_apply')
      return { id: 'mutation-1', state: 'Applied' };
    if (operation === 'edit_readback' || operation === 'edit_discard')
      return [];
    return null;
  });
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  const view = render(
    <TableEditor
      transport={{ request } as unknown as DatabaseTransport}
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
  await screen.findByText('UPDATE customers SET label');
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  await waitFor(() =>
    expect(
      request.mock.calls.filter(([operation]) => operation === 'edit_snapshot'),
    ).toHaveLength(2),
  );
  expect(useEditDrafts.getState().drafts['table-tab']?.state).toBe(
    'Refreshing',
  );
  view.unmount();
  render(
    <TableEditor
      transport={{ request } as unknown as DatabaseTransport}
      connection="connection-1"
      object={object}
      tabId="table-tab"
      onExit={() => {}}
    />,
  );

  const input = screen.getByRole('textbox', { name: 'label baris 1' });
  expect(input).toBeDisabled();
  expect(screen.getByRole('button', { name: 'Apply' })).toBeDisabled();
  fireEvent.change(input, { target: { value: 'Must not overwrite' } });
  expect(input).toHaveValue('Original label');

  resolveRefresh(refreshed);
  await waitFor(() =>
    expect(screen.getByRole('textbox', { name: 'label baris 1' })).toHaveValue(
      'Updated label',
    ),
  );
  expect(screen.getByRole('textbox', { name: 'label baris 1' })).toBeEnabled();
  useEditDrafts.setState({ drafts: {} });
  vi.restoreAllMocks();
});
