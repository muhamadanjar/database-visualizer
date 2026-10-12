import { useEffect, useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import type {
  DatabaseTransport,
  Execution,
  Result,
  Row,
} from '@database-studio/contracts';
import { describeError } from '@database-studio/contracts';
import { resultCache } from './cache';
export const csv = (rows: Row[]) =>
  rows
    .map((row) =>
      row
        .map((cell) => `"${(cell.value ?? '').replaceAll('"', '""')}"`)
        .join(','),
    )
    .join('\r\n');
export function ResultGrid({
  transport,
  execution,
  result,
  owner,
  onSort,
  onTransform,
}: {
  transport: DatabaseTransport;
  execution: Execution;
  result: Result;
  owner: string;
  onSort?: (column: string) => void;
  onTransform?: (execution: Execution) => void;
}) {
  const [filter, setFilter] = useState('');
  const [sorting, setSorting] = useState<{
    column: number;
    descending: boolean;
  } | null>(null);
  const [viewRevision, setViewRevision] = useState(0);
  const [transforming, setTransforming] = useState(false);
  const transform = async (
    column: number | null,
    descending = false,
    text = filter,
  ) => {
    if (!onTransform) return;
    setTransforming(true);
    try {
      const updated = await transport.request<Execution>('transform_result', {
        execution_id: execution.id,
        result_id: result.id,
        column,
        descending,
        filter: text,
      });
      resultCache.clear(owner);
      setViewRevision((v) => v + 1);
      setSelection(null);
      setSelectedRows(new Set());
      onTransform(updated);
    } catch (e) {
      setMessage(describeError(e));
    } finally {
      setTransforming(false);
    }
  };
  const scroll = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<{
    row: number;
    column: number;
  } | null>(null);
  const [selectedRows, setSelectedRows] = useState<Set<number>>(new Set());
  const [message, setMessage] = useState('');
  const virtual = useVirtualizer({
    count: result.rows,
    getScrollElement: () => scroll.current,
    estimateSize: () => 28,
    overscan: 8,
  });
  const columns = useVirtualizer({
    count: result.columns.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 160,
    horizontal: true,
    overscan: 2,
    paddingStart: 44,
  });
  const visible = virtual.getVirtualItems();
  const offset = Math.floor((visible[0]?.index ?? 0) / 128) * 128;
  const query = useQuery({
    queryKey: ['page', result.id, offset, result.rows, viewRevision],
    enabled: result.rows > 0,
    gcTime: 0,
    queryFn: async () => {
      const key = `${result.id}/${offset}/${result.rows}/${viewRevision}`;
      const cached = resultCache.get(key);
      if (cached) return cached;
      const rows = await transport.request<Row[]>('page', {
        execution_id: execution.id,
        result_id: result.id,
        offset,
        limit: 256,
      });
      resultCache.put(key, owner, rows);
      return rows;
    },
  });
  useEffect(() => {
    setSelection(null);
    setSelectedRows(new Set());
  }, [result.id]);
  const getRow = (index: number) => query.data?.[index - offset];
  const copy = async (kind: 'cell' | 'rows' | 'csv') => {
    try {
      let text = '';
      if (kind === 'cell' && selection) {
        const row = await transport.request<Row[]>('page', {
          execution_id: execution.id,
          result_id: result.id,
          offset: selection.row,
          limit: 1,
        });
        if (!row[0]) throw new Error('Row tidak tersedia.');
        text = row[0][selection.column]?.value ?? '';
      } else {
        const indexes = [...selectedRows].sort((a, b) => a - b);
        const rows: Row[] = [];
        let bytes = 0;
        let pageOffset = -1;
        let page: Row[] = [];
        for (const index of indexes) {
          if (index < pageOffset || index >= pageOffset + page.length) {
            pageOffset = index;
            page = await transport.request<Row[]>('page', {
              execution_id: execution.id,
              result_id: result.id,
              offset: index,
              limit: 256,
            });
          }
          const row = page[index - pageOffset];
          if (!row) throw new Error('Row terpilih tidak tersedia.');
          bytes += new TextEncoder().encode(JSON.stringify(row)).byteLength;
          if (bytes > 32 * 1024 * 1024)
            throw new Error('Copy dibatasi 32 MiB. Pilih lebih sedikit rows.');
          rows.push(row);
        }
        text =
          kind === 'csv'
            ? csv(rows)
            : rows
                .map((row) => row.map((c) => c.value ?? 'NULL').join('\t'))
                .join('\n');
      }
      await transport.request('copy_text', { text });
      setMessage('Disalin.');
    } catch (e) {
      setMessage(describeError(e));
    }
  };
  const complete =
    result.terminal?.type === 'query_completed' &&
    result.terminal.summary?.results_complete;
  return (
    <div className="result-area">
      <div className="grid-tools">
        <span>
          {result.rows.toLocaleString('id-ID')} rows
          {complete ? '' : ' · hasil parsial / sedang diterima'}
        </span>
        <div>
          <button disabled={!selection} onClick={() => void copy('cell')}>
            Copy cell
          </button>
          <button
            disabled={!selectedRows.size}
            onClick={() => void copy('rows')}
          >
            Copy rows
          </button>
          <button
            disabled={!selectedRows.size}
            onClick={() => void copy('csv')}
          >
            Copy CSV
          </button>
        </div>
        <span className="muted">{message}</span>
      </div>
      {onTransform ? (
        <div className="table-controls">
          <input
            aria-label="Filter hasil diterima"
            value={filter}
            maxLength={4096}
            placeholder="Filter rows yang sudah diterima…"
            onChange={(e) => setFilter(e.target.value)}
          />
          <button
            disabled={!execution.finished_at_ms || transforming}
            onClick={() =>
              void transform(
                sorting?.column ?? null,
                sorting?.descending,
                filter,
              )
            }
          >
            Terapkan
          </button>
          <button
            disabled={!execution.finished_at_ms || transforming}
            onClick={() => {
              setFilter('');
              setSorting(null);
              void transform(null, false, '');
            }}
          >
            Reset
          </button>
          <span className="muted small">
            Klik header untuk sorting hasil lokal; SQL asli tetap.
          </span>
        </div>
      ) : null}
      {query.error ? (
        <p role="alert" className="error-text">
          {describeError(query.error)}
        </p>
      ) : null}
      <div
        ref={scroll}
        className="grid-scroll"
        role="grid"
        aria-label="Hasil query"
        aria-rowcount={result.rows}
        aria-colcount={result.columns.length}
      >
        <div
          style={{
            width: Math.max(columns.getTotalSize(), 300),
            minHeight: '100%',
          }}
        >
          <div
            className="grid-header"
            style={{ width: columns.getTotalSize() }}
          >
            <span className="row-number">#</span>
            {columns.getVirtualItems().map((column) => (
              <button
                key={column.key}
                className="grid-column"
                style={{
                  position: 'absolute',
                  left: column.start,
                  width: column.size,
                }}
                title={`${result.columns[column.index].name}: ${result.columns[column.index].database_type}`}
                disabled={
                  transforming || (!onSort && !execution.finished_at_ms)
                }
                onClick={() => {
                  if (onSort) onSort(result.columns[column.index].name);
                  else if (onTransform) {
                    const descending =
                      sorting?.column === column.index
                        ? !sorting.descending
                        : false;
                    setSorting({ column: column.index, descending });
                    void transform(column.index, descending);
                  }
                }}
              >
                {result.columns[column.index].name}
                <small>{result.columns[column.index].database_type}</small>
              </button>
            ))}
          </div>
          <div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
            {visible.map((row) => {
              const data = getRow(row.index);
              return (
                <div
                  key={row.key}
                  className={`grid-row${selectedRows.has(row.index) ? ' selected-row' : ''}`}
                  role="row"
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    height: row.size,
                    width: columns.getTotalSize(),
                    transform: `translateY(${row.start}px)`,
                  }}
                >
                  <button
                    className="row-number"
                    aria-label={`Pilih row ${row.index + 1}`}
                    onClick={() =>
                      setSelectedRows((previous) => {
                        const next = new Set(previous);
                        if (next.has(row.index)) next.delete(row.index);
                        else if (next.size < 10000) next.add(row.index);
                        else setMessage('Pilih maksimal 10.000 rows.');
                        return next;
                      })
                    }
                  >
                    {row.index + 1}
                  </button>
                  {columns.getVirtualItems().map((column) => {
                    const cell = data?.[column.index];
                    return (
                      <button
                        key={column.key}
                        role="gridcell"
                        className={`grid-cell${cell?.kind === 'null' ? ' null-cell' : ''}${selection?.row === row.index && selection.column === column.index ? ' selected-cell' : ''}`}
                        style={{
                          position: 'absolute',
                          left: column.start,
                          width: column.size,
                        }}
                        title={cell?.value ?? 'NULL'}
                        onClick={() =>
                          setSelection({ row: row.index, column: column.index })
                        }
                      >
                        {data ? (cell?.value ?? 'NULL') : '…'}
                      </button>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      </div>
      <div className="grid-status">
        <span>
          {selectedRows.size
            ? `${selectedRows.size} rows dipilih`
            : 'Klik cell atau nomor row untuk memilih'}
        </span>
        <span>
          {result.terminal ? `${result.terminal.elapsed_ms} ms` : 'Streaming…'}
        </span>
      </div>
    </div>
  );
}
