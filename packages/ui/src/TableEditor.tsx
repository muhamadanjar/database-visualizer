import { useEffect, useState } from 'react';
import type { DatabaseTransport, ObjectId } from '@database-studio/contracts';
import { describeError } from '@database-studio/contracts';
import {
  useEditDrafts,
  discardEditDrafts,
  type Change,
  type Input,
  type EditSnapshot,
} from './editDrafts';
const supported = (type: string) =>
  /^(smallint|integer|bigint|int|tinyint|mediumint|numeric|decimal|real|double|float|boolean|bool|text|character|varchar|char|tinytext|mediumtext|longtext|bytea|binary|varbinary|blob|tinyblob|mediumblob|longblob|json|jsonb|date|time|timestamp|datetime)([ (]|$)/i.test(
    type,
  ) && !type.includes('with time zone');
export function TableEditor({
  transport,
  connection,
  object,
  tabId,
}: {
  transport: DatabaseTransport;
  connection: string;
  object: ObjectId;
  tabId: string;
}) {
  const draft = useEditDrafts((s) => s.drafts[tabId]);
  const put = useEditDrafts((s) => s.put);
  const [readback, setReadback] = useState<EditSnapshot['rows']>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const load = async (offset = draft?.snapshot.offset ?? 0) => {
    setLoading(true);
    setError('');
    try {
      if (draft && !(await discardEditDrafts(transport, tabId))) return;
      const snapshot = await transport.request<EditSnapshot>('edit_snapshot', {
        connection_id: connection,
        tab_id: tabId,
        object,
        offset,
      });
      put(tabId, { connection, snapshot, changes: [], state: 'Draft' });
    } catch (e) {
      setError(describeError(e));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    if (!draft) void load();
  }, [connection, tabId]);
  const revise = (changes: Change[]) => {
    if (draft)
      put(tabId, {
        ...draft,
        changes,
        preview: undefined,
        state: 'Draft',
        key: undefined,
        mutation: undefined,
      });
  };
  const cell = (row: number, name: string, value: Input) => {
    if (!draft) return;
    const changes = draft.changes.filter(
      (c) => !(c.type === 'update' && c.row === row),
    );
    const previous = draft.changes.find(
      (c) => c.type === 'update' && c.row === row,
    );
    revise([
      ...changes,
      {
        type: 'update',
        row,
        values: {
          ...(previous?.type === 'update' ? previous.values : {}),
          [name]: value,
        },
      },
    ]);
  };
  const preview = async () => {
    if (!draft) return;
    try {
      const value = await transport.request<{
        statements: string[];
        warnings: string[];
      }>('edit_preview', {
        connection_id: connection,
        tab_id: tabId,
        batch: {
          tab_id: tabId,
          token: draft.snapshot.token,
          changes: draft.changes,
        },
      });
      put(tabId, { ...draft, preview: value });
    } catch (e) {
      setError(describeError(e));
    }
  };
  const apply = async () => {
    if (
      !draft?.preview ||
      !window.confirm(
        'Terapkan ' +
          draft.changes.length +
          ' perubahan dalam transaksi khusus? Constraint, cascade dan trigger database tetap berlaku.',
      )
    )
      return;
    const key = crypto.randomUUID();
    put(tabId, { ...draft, key, state: 'Applying' });
    setError('');
    try {
      let result = await transport.request<{ id: string; state: string }>(
        'edit_apply',
        {
          connection_id: connection,
          tab_id: tabId,
          idempotency_key: key,
          batch: {
            tab_id: tabId,
            token: draft.snapshot.token,
            changes: draft.changes,
          },
        },
      );
      put(tabId, { ...draft, key, mutation: result.id, state: result.state });
      while (result.state === 'Applying') {
        await new Promise((resolve) => setTimeout(resolve, 500));
        result = await transport.request('edit_status', {
          connection_id: connection,
          tab_id: tabId,
          mutation_id: result.id,
          idempotency_key: key,
        });
        put(tabId, { ...draft, key, mutation: result.id, state: result.state });
      }
      if (result.state === 'Applied') {
        try {
          setReadback(
            await transport.request<EditSnapshot['rows']>('edit_readback', {
              connection_id: connection,
              tab_id: tabId,
              token: draft.snapshot.token,
            }),
          );
        } catch {
          setError(
            'Apply terkonfirmasi. Readback tidak tersedia; periksa data melalui query.',
          );
        }

        put(tabId, {
          ...draft,
          key,
          mutation: result.id,
          state: 'Applied',
          changes: [],
        });
        await transport.request('edit_discard', {
          connection_id: connection,
          tab_id: tabId,
        });
        const snapshot = await transport.request<EditSnapshot>(
          'edit_snapshot',
          {
            connection_id: connection,
            tab_id: tabId,
            object,
            offset: draft.snapshot.offset,
          },
        );
        put(tabId, { connection, snapshot, changes: [], state: 'Applied' });
      }
    } catch (e) {
      const latest = useEditDrafts.getState().drafts[tabId];
      if (latest && latest.state !== 'Applied')
        put(tabId, { ...latest, state: 'Unknown' });
      setError(describeError(e));
    }
  };
  const locked =
    !draft ||
    draft.state === 'Applying' ||
    ['Unknown', 'Conflict', 'Failed'].includes(draft.state);
  if (!draft)
    return (
      <div>
        <button disabled={loading} onClick={() => void load()}>
          Muat halaman editing
        </button>
        {error ? <p role="alert">{error}</p> : null}
      </div>
    );
  const inserts = draft.changes.filter(
    (c): c is Extract<Change, { type: 'insert' }> => c.type === 'insert',
  );
  return (
    <section className="table-editor">
      <div className="table-controls">
        <button
          disabled={loading || draft.state === 'Applying'}
          onClick={() => void load()}
        >
          Refresh / buang draft
        </button>
        <button
          disabled={
            loading || draft.state === 'Applying' || !draft.snapshot.offset
          }
          onClick={() => void load(Math.max(0, draft.snapshot.offset - 100))}
        >
          Halaman sebelumnya
        </button>
        <button
          disabled={
            loading ||
            draft.state === 'Applying' ||
            draft.snapshot.rows.length < 100 ||
            draft.snapshot.offset >= 100000
          }
          onClick={() => void load(draft.snapshot.offset + 100)}
        >
          Halaman berikutnya
        </button>
        <button
          disabled={locked || draft.changes.length >= 100}
          onClick={() =>
            revise([...draft.changes, { type: 'insert', values: {} }])
          }
        >
          Tambah baris
        </button>
        <button
          disabled={locked || !draft.changes.length}
          onClick={() => void preview()}
        >
          Preview
        </button>
        <button
          disabled={locked || !draft.preview}
          onClick={() => void apply()}
        >
          Apply
        </button>
        <button
          disabled={draft.state !== 'Applying' || !draft.mutation}
          onClick={() =>
            void transport
              .request('edit_cancel', {
                connection_id: connection,
                tab_id: tabId,
                mutation_id: draft.mutation,
                idempotency_key: draft.key,
              })
              .catch((e) => setError(describeError(e)))
          }
        >
          Cancel Apply
        </button>
        <span>
          {draft.changes.length}/100 perubahan · {draft.state}
        </span>
      </div>
      <p className="small">
        Halaman {1 + draft.snapshot.offset / 100}, maksimal 100 baris menurut
        primary key. Snapshot sementara 10 menit. Transaksi tab SQL terpisah;
        perubahan tidak autosave.
      </p>
      {readback.length && draft.state === 'Applied' ? (
        <details open>
          <summary>Readback baris yang diterapkan</summary>
          <pre>
            {readback
              .map((row) =>
                row
                  .map(
                    (cell, i) =>
                      `${draft.snapshot.columns[i]?.name}: ${cell.value ?? 'NULL'}`,
                  )
                  .join(' · '),
              )
              .join('\n')}
          </pre>
        </details>
      ) : null}
      {draft.state === 'Unknown' ? (
        <p role="alert">
          Commit tidak dapat dipastikan. Periksa data melalui query sebelum
          membuat perubahan baru. Apply tidak diulang otomatis.
        </p>
      ) : null}
      {['Conflict', 'Failed'].includes(draft.state) ? (
        <p role="alert">
          Batch tidak diterapkan atau mengalami konflik. Refresh dan review
          ulang; jika rollback tidak pasti, status menjadi Unknown.
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="error-text">
          {error}
        </p>
      ) : null}
      <div className="definition-scroll">
        <table className="definition-table">
          <thead>
            <tr>
              <th>Aksi</th>
              {draft.snapshot.columns.map((c) => (
                <th key={c.name}>
                  {c.name}
                  <small> {c.data_type}</small>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {draft.snapshot.rows.map((row, i) => {
              const deleted = draft.changes.some(
                (c) => c.type === 'delete' && c.row === i,
              );
              const update = draft.changes.find(
                (c) => c.type === 'update' && c.row === i,
              );
              return (
                <tr key={i} style={{ opacity: deleted ? 0.4 : 1 }}>
                  <td>
                    <button
                      disabled={locked}
                      onClick={() =>
                        revise([
                          ...draft.changes.filter(
                            (c) => !('row' in c && c.row === i),
                          ),
                          ...(deleted
                            ? []
                            : [{ type: 'delete' as const, row: i }]),
                        ])
                      }
                    >
                      {deleted ? 'Batal hapus' : 'Hapus'}
                    </button>
                  </td>
                  {draft.snapshot.columns.map((c, j) => {
                    const value =
                      update?.type === 'update'
                        ? update.values[c.name]
                        : undefined;
                    const readOnly =
                      draft.snapshot.primary_key.includes(c.name) ||
                      draft.snapshot.generated.includes(c.name) ||
                      !supported(c.data_type);
                    return (
                      <td key={c.name}>
                        <input
                          aria-label={c.name + ' baris ' + (i + 1)}
                          readOnly={readOnly}
                          disabled={locked || deleted}
                          value={
                            value?.type === 'value'
                              ? (value.value ?? '')
                              : value?.type === 'default'
                                ? 'DEFAULT'
                                : (row[j]?.value ?? '')
                          }
                          onChange={(e) =>
                            cell(i, c.name, {
                              type: 'value',
                              value: e.target.value,
                            })
                          }
                        />
                        {!readOnly ? (
                          <>
                            <button
                              disabled={locked || deleted || !c.nullable}
                              onClick={() =>
                                cell(i, c.name, { type: 'value', value: null })
                              }
                            >
                              NULL
                            </button>
                            <button
                              disabled={locked || deleted}
                              onClick={() =>
                                cell(i, c.name, { type: 'default' })
                              }
                            >
                              Default
                            </button>
                            <span>
                              {value?.type === 'default'
                                ? 'DEFAULT'
                                : value?.type === 'value' &&
                                    value.value === null
                                  ? 'NULL'
                                  : row[j]?.kind === 'null' && !value
                                    ? 'NULL'
                                    : ''}
                            </span>
                          </>
                        ) : null}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
            {inserts.map((change, i) => (
              <tr key={'new' + i}>
                <td>Baris baru {i + 1}</td>
                {draft.snapshot.columns.map((c) => (
                  <td key={c.name}>
                    {draft.snapshot.generated.includes(c.name) ||
                    !supported(c.data_type) ? (
                      'Generated / read-only'
                    ) : (
                      <>
                        <input
                          aria-label={c.name + ' baru ' + (i + 1)}
                          disabled={locked}
                          placeholder="DEFAULT"
                          value={
                            change.values[c.name]?.type === 'value'
                              ? ((
                                  change.values[c.name] as Extract<
                                    Input,
                                    { type: 'value' }
                                  >
                                ).value ?? '')
                              : ''
                          }
                          onChange={(e) =>
                            revise(
                              draft.changes.map((item) =>
                                item === change
                                  ? {
                                      ...change,
                                      values: {
                                        ...change.values,
                                        [c.name]: {
                                          type: 'value',
                                          value: e.target.value,
                                        },
                                      },
                                    }
                                  : item,
                              ),
                            )
                          }
                        />
                        <button
                          disabled={locked || !c.nullable}
                          onClick={() =>
                            revise(
                              draft.changes.map((item) =>
                                item === change
                                  ? {
                                      ...change,
                                      values: {
                                        ...change.values,
                                        [c.name]: {
                                          type: 'value',
                                          value: null,
                                        },
                                      },
                                    }
                                  : item,
                              ),
                            )
                          }
                        >
                          NULL
                        </button>
                        <button
                          disabled={locked}
                          onClick={() =>
                            revise(
                              draft.changes.map((item) =>
                                item === change
                                  ? {
                                      ...change,
                                      values: {
                                        ...change.values,
                                        [c.name]: { type: 'default' },
                                      },
                                    }
                                  : item,
                              ),
                            )
                          }
                        >
                          Default
                        </button>
                        <span>
                          {change.values[c.name]?.type === 'default' ||
                          !change.values[c.name]
                            ? 'DEFAULT'
                            : change.values[c.name]?.type === 'value' &&
                                (
                                  change.values[c.name] as Extract<
                                    Input,
                                    { type: 'value' }
                                  >
                                ).value === null
                              ? 'NULL'
                              : ''}
                        </span>
                      </>
                    )}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {draft.preview ? (
        <pre className="ddl">
          {draft.preview.statements.join(';\n')}
          <br />
          {draft.preview.warnings?.join('\n')}
        </pre>
      ) : null}
    </section>
  );
}
