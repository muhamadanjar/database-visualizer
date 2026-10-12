import { useState } from 'react';
import type { DatabaseTransport } from '@database-studio/contracts';
export function DefinitionText({
  text,
  transport,
}: {
  text: string;
  transport: DatabaseTransport;
}) {
  const [query, setQuery] = useState('');
  const [index, setIndex] = useState(0);
  const [error, setError] = useState('');
  const needle = query.toLowerCase();
  const positions: number[] = [];
  let offset = 0;
  if (needle)
    while (positions.length < 1000) {
      const found = text.toLowerCase().indexOf(needle, offset);
      if (found < 0) break;
      positions.push(found);
      offset = found + needle.length;
    }
  const selected = positions.length ? positions[index % positions.length] : -1;
  return (
    <section>
      <div className="table-controls">
        <input
          aria-label="Cari definisi"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIndex(0);
          }}
        />
        <button
          disabled={!positions.length}
          onClick={() =>
            setIndex((v) => (v + positions.length - 1) % positions.length)
          }
        >
          Previous
        </button>
        <button
          disabled={!positions.length}
          onClick={() => setIndex((v) => v + 1)}
        >
          Next
        </button>
        <span>
          {positions.length
            ? 'Hasil ' +
              ((index % positions.length) + 1) +
              '/' +
              positions.length
            : 'Tidak ada kecocokan'}
        </span>
        <button
          onClick={() =>
            void transport
              .request('copy_text', { text })
              .catch(() =>
                setError(
                  'Clipboard tidak tersedia. Pilih teks dan copy manual.',
                ),
              )
          }
        >
          Copy
        </button>
      </div>
      {error ? <p role="alert">{error}</p> : null}
      <pre className="ddl">
        {selected < 0 ? (
          text
        ) : (
          <>
            {text.slice(0, selected)}
            <mark>{text.slice(selected, selected + query.length)}</mark>
            {text.slice(selected + query.length)}
          </>
        )}
      </pre>
    </section>
  );
}
