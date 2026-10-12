import { useState, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useVirtualizer } from '@tanstack/react-virtual';
import type {
  DatabaseTransport,
  Metadata,
  ObjectId,
  ObjectKind,
  Profile,
} from '@database-studio/contracts';
import { Icon } from './Icon';
import { describeError } from '@database-studio/contracts';
const labels: Partial<Record<ObjectKind, string>> = {
  Table: 'Tables',
  View: 'Views',
  MaterializedView: 'Materialized views',
  Sequence: 'Sequences',
  Function: 'Functions',
  Procedure: 'Procedures',
  Trigger: 'Triggers',
  CustomType: 'Types',
};
function Group({
  transport,
  profile,
  schema,
  kind,
  onOpen,
  onNames,
}: {
  transport: DatabaseTransport;
  profile: Profile;
  schema: string | null;
  kind: ObjectKind;
  onOpen: (object: ObjectId) => void;
  onNames: (names: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const scroll = useRef<HTMLDivElement>(null);
  const query = useQuery({
    queryKey: ['objects', profile.id, profile.revision, schema, kind],
    enabled: open,
    queryFn: async () => {
      const result = await transport.request<Metadata>('metadata', {
        connection_id: profile.id,
        request: { Objects: { schema, kind } },
      });
      onNames(result.objects.map((o) => o.name));
      return result;
    },
    staleTime: 30000,
  });
  const objects = query.data?.objects ?? [];
  const virtual = useVirtualizer({
    count: objects.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 28,
    overscan: 5,
  });
  return (
    <div className="tree-group">
      <button
        className="tree-line"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="chevron">
          <Icon name="chevron" open={open} />
        </span>
        <Icon name="folder" />
        {labels[kind]}
        {open && query.data ? (
          <span className="count">{objects.length}</span>
        ) : null}
      </button>
      {open ? (
        <div className="tree-children">
          {query.isPending ? (
            <p className="muted small">Memuat…</p>
          ) : query.error ? (
            <p role="alert" className="small error-text">
              {describeError(query.error)}
            </p>
          ) : objects.length ? (
            <div
              ref={scroll}
              style={{
                height: Math.min(280, objects.length * 28),
                overflow: 'auto',
              }}
            >
              <div
                style={{ height: virtual.getTotalSize(), position: 'relative' }}
              >
                {virtual.getVirtualItems().map((item) => (
                  <button
                    key={item.key}
                    className="tree-object"
                    style={{
                      position: 'absolute',
                      top: 0,
                      left: 0,
                      width: '100%',
                      height: item.size,
                      transform: `translateY(${item.start}px)`,
                    }}
                    title={objects[item.index].name}
                    onClick={() => onOpen(objects[item.index])}
                  >
                    <Icon name="table" />
                    {objects[item.index].name}
                  </button>
                ))}
              </div>
            </div>
          ) : (
            <p className="muted small">Tidak ada object.</p>
          )}
        </div>
      ) : null}
    </div>
  );
}
function Schema({
  name,
  transport,
  profile,
  onOpen,
  onNames,
}: {
  name: string;
  transport: DatabaseTransport;
  profile: Profile;
  onOpen: (object: ObjectId) => void;
  onNames: (names: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        className="tree-line schema-line"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="chevron">
          <Icon name="chevron" open={open} />
        </span>
        <Icon name="database" /> {name}
      </button>
      {open ? (
        <div className="schema-children">
          {Object.keys(labels).map((kind) => (
            <Group
              key={kind}
              transport={transport}
              profile={profile}
              schema={name}
              kind={kind as ObjectKind}
              onOpen={onOpen}
              onNames={onNames}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
export function Explorer({
  transport,
  profile,
  onOpen,
  onNames,
}: {
  transport: DatabaseTransport;
  profile: Profile;
  onOpen: (object: ObjectId) => void;
  onNames: (names: string[]) => void;
}) {
  const schemas = useQuery({
    queryKey: ['schemas', profile.id, profile.revision],
    enabled: profile.engine === 'Postgres',
    queryFn: () =>
      transport.request<Metadata>('metadata', {
        connection_id: profile.id,
        request: 'Schemas',
      }),
    staleTime: 30000,
  });
  return (
    <div className="explorer-tree">
      <div className="tree-line database-line">
        <span className="engine-glyph">
          {profile.engine === 'Postgres' ? 'P' : 'M'}
        </span>
        <strong>{profile.database}</strong>
      </div>
      {profile.engine === 'Postgres' ? (
        schemas.error ? (
          <p className="small error-text">{describeError(schemas.error)}</p>
        ) : schemas.isPending ? (
          <p className="muted small">Memuat schemas…</p>
        ) : (
          schemas.data.objects.map((o) => (
            <Schema
              key={o.name}
              name={o.name}
              transport={transport}
              profile={profile}
              onOpen={onOpen}
              onNames={onNames}
            />
          ))
        )
      ) : (
        <div className="schema-children">
          {(
            [
              'Table',
              'View',
              'Function',
              'Procedure',
              'Trigger',
            ] as ObjectKind[]
          ).map((kind) => (
            <Group
              key={kind}
              transport={transport}
              profile={profile}
              schema={null}
              kind={kind}
              onOpen={onOpen}
              onNames={onNames}
            />
          ))}
        </div>
      )}
      <p className="tree-note">
        Metadata memakai koneksi terpisah; temporary table editor tidak
        ditampilkan.
      </p>
    </div>
  );
}
