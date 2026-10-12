import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import {
  QueryClient,
  QueryClientProvider,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';
import type {
  DatabaseTransport,
  Document,
  DocumentHeader,
  Execution,
  Metadata,
  ObjectId,
  Profile,
  Session,
} from '@database-studio/contracts';
import { describeError, terminal } from '@database-studio/contracts';
import { useWorkspace, type Tab } from './store';
import { ConnectionDialog } from './ConnectionDialog';
import { Explorer } from './Explorer';
import { Icon } from './Icon';
import { ResultGrid } from './ResultGrid';
import { resultCache } from './cache';
import { TableEditor } from './TableEditor';
import { enforceMetadataBudget } from './metadataBudget';
import { DefinitionText } from './DefinitionText';
import { discardEditDrafts, editingDirty } from './editDrafts';
import type { EditorHandle } from './SqlEditor';
const SqlEditor = lazy(() => import('./SqlEditor'));
const makeQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        refetchOnWindowFocus: false,
        gcTime: 300000,
        staleTime: 300000,
      },
    },
  });
export function Workspace({ transport }: { transport: DatabaseTransport }) {
  const [queryClient] = useState(makeQueryClient);
  useEffect(() => enforceMetadataBudget(queryClient), [queryClient]);
  return (
    <QueryClientProvider client={queryClient}>
      <Workbench transport={transport} />
    </QueryClientProvider>
  );
}

function Workbench({ transport }: { transport: DatabaseTransport }) {
  const query = useQueryClient();
  const state = useWorkspace();
  const canManage = transport.capabilities?.manageConnections ?? true;
  const canExecute = transport.capabilities?.execute ?? true;
  const isWeb = transport.capabilities?.platform === 'web';
  const { tabs, activeId, add, patch, remove, activate } = state;
  const active = tabs.find((t) => t.id === activeId);
  const [connectionId, setConnectionId] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Profile | 'new' | null>(null);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [completions, setCompletions] = useState<string[]>([]);
  const completionScope = useRef('');
  const completionUpdated = useRef(0);
  const addNames = (id: string, revision: number, names: string[]) => {
    const scope = id + '/' + revision;
    if (
      completionScope.current !== scope ||
      Date.now() - completionUpdated.current > 300000
    ) {
      completionScope.current = scope;
      setCompletions([]);
    }
    completionUpdated.current = Date.now();
    setCompletions((previous) => {
      const merged = [...new Set([...previous, ...names])];
      let bytes = 0;
      return merged.filter((name) => {
        bytes += new TextEncoder().encode(name).length;
        return bytes <= 32 * 1024 * 1024;
      });
    });
  };
  const [resultIndex, setResultIndex] = useState(0);
  const [bottomTab, setBottomTab] = useState<
    'results' | 'messages' | 'explain'
  >('results');
  const [documents, setDocuments] = useState<DocumentHeader[] | null>(null);
  const [documentKind, setDocumentKind] = useState<'history' | 'saved'>(
    'saved',
  );
  const editor = useRef<EditorHandle | null>(null);
  const watchers = useRef(new Map<string, () => void>());
  const running = active?.execution && !terminal(active.execution.status);
  const profiles = useQuery({
    queryKey: ['profiles'],
    queryFn: async () => {
      await transport.request('handshake', { version: 1 });
      return transport.request<Profile[]>('profiles');
    },
  });
  const profile = profiles.data?.find(
    (p) => p.id === (active?.connectionId ?? connectionId),
  );
  const selected = profiles.data?.find((p) => p.id === connectionId);
  const connected = !canExecute
    ? !!connectionId
    : tabs.some((t) => t.connectionId === connectionId && t.session?.valid);
  const showError = (e: unknown) => setError(describeError(e));
  useEffect(() => {
    void transport
      .request<{ sidebar_width?: number; editor_height?: number }>('layout')
      .then((layout) =>
        useWorkspace.getState().layout({
          sidebarWidth: layout.sidebar_width ?? 248,
          editorHeight: layout.editor_height ?? 330,
        }),
      )
      .catch(() => {});
    return () => {
      for (const dispose of watchers.current.values()) dispose();
    };
  }, [transport]);
  const restored = useRef(new Set<string>());
  const saving = useRef<Promise<void>>(Promise.resolve());
  const closingTabs = useRef(new Set<string>());
  const savedText = useRef(new Map<string, string>());
  const saveEditors = () => {
    const job = saving.current
      .catch(() => {})
      .then(async () => {
        for (const tab of useWorkspace.getState().tabs) {
          const fingerprint = JSON.stringify([tab.title, tab.sql]);
          if (
            tab.inspector ||
            closingTabs.current.has(tab.id) ||
            savedText.current.get(tab.id) === fingerprint ||
            !profiles.data?.some(
              (p) => p.id === tab.connectionId && p.restore_sql,
            )
          )
            continue;
          await transport.request('save_document', {
            document: {
              id: tab.id,
              connection_id: tab.connectionId,
              kind: 'editor',
              name: tab.title,
              content: { sql: tab.sql },
              created_at_ms: Date.now(),
            },
          });
          savedText.current.set(tab.id, fingerprint);
        }
      });
    saving.current = job;
    return job;
  };
  const closeApp = useRef<() => Promise<boolean>>(async () => true);
  closeApp.current = async () => {
    if (!(await discardEditDrafts(transport))) return false;
    try {
      const current = useWorkspace.getState().tabs;
      const sessions = await Promise.all(
        current
          .filter((t) => t.session)
          .map((t) =>
            transport.request<Session>('session', {
              session_id: t.session!.id,
            }),
          ),
      );
      if (
        (sessions.some((s) => s.busy || s.transaction !== 'Idle') ||
          current.some((t) => !t.inspector && t.sql.trim())) &&
        !window.confirm(
          'Tutup workspace? Query aktif akan dibatalkan dan transaksi aktif/tidak diketahui di-disconnect. Teks SQL hanya dipulihkan untuk koneksi dengan restore aktif; simpan query yang diperlukan.',
        )
      )
        return false;
      await saveEditors();
      return true;
    } catch (e) {
      showError(e);
      return false;
    }
  };
  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void transport
      .onCloseRequested?.(() => closeApp.current())
      .then((fn) => {
        if (disposed) fn();
        else unsubscribe = fn;
      });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [transport]);
  useEffect(() => {
    const handler = (event: BeforeUnloadEvent) => {
      if (editingDirty()) {
        event.preventDefault();
        event.returnValue = '';
      }
    };
    window.addEventListener('beforeunload', handler);
    return () => window.removeEventListener('beforeunload', handler);
  }, []);
  useEffect(() => {
    const timer = setTimeout(() => void saveEditors().catch(showError), 750);
    return () => clearTimeout(timer);
  }, [tabs, profiles.data, transport]);
  useEffect(() => {
    if (!canExecute) return;
    for (const p of profiles.data ?? []) {
      if (!p.restore_sql || restored.current.has(p.id)) continue;
      restored.current.add(p.id);
      void transport
        .request<DocumentHeader[]>('documents', {
          connection_id: p.id,
          kind: 'editor',
        })
        .then(async (docs) => {
          for (const header of docs) {
            const doc = await transport.request<Document>('document', {
              connection_id: header.connection_id,
              kind: header.kind,
              id: header.id,
            });
            if (!useWorkspace.getState().tabs.some((t) => t.id === doc.id))
              add({
                id: doc.id,
                connectionId: p.id,
                title: doc.name,
                sql: doc.content.sql,
              });
          }
          if (docs.length) {
            setConnectionId(p.id);
            setNotice(
              'Teks editor dipulihkan; session dan SQL tidak dijalankan otomatis.',
            );
          }
        })
        .catch(showError);
    }
  }, [profiles.data, transport, add]);
  const newTab = (id = connectionId) => {
    if (!canExecute) return;
    if (!id) {
      setDialog('new');
      return;
    }
    const p = profiles.data?.find((p) => p.id === id);
    add({
      id: crypto.randomUUID(),
      connectionId: id,
      title: 'Query baru',
      sql: p?.engine === 'MySql' ? 'SELECT VERSION();' : 'SELECT version();',
    });
    setConnectionId(id);
    setDocuments(null);
  };
  const ensureSession = async (tab: Tab) => {
    if (tab.session?.valid) return tab.session;
    const session = await transport.request<Session>('connect', {
      connection_id: tab.connectionId,
    });
    patch(tab.id, { session });
    return session;
  };
  const refresh = useCallback(
    async (tabId: string, id: string) => {
      try {
        const execution = await transport.request<Execution>('execution', {
          execution_id: id,
        });
        const current = useWorkspace
          .getState()
          .tabs.find((t) => t.id === tabId);
        if (current?.execution?.id !== id) return;
        patch(tabId, { execution });
        if (terminal(execution.status)) {
          watchers.current.get(tabId)?.();
          watchers.current.delete(tabId);
          const session = await transport.request<Session>('session', {
            session_id: execution.session_id,
          });
          patch(tabId, { session });
        }
      } catch (e) {
        showError(e);
      }
    },
    [transport, patch],
  );
  const attach = async (tab: Tab, id: string, session: Session) => {
    const execution = await transport.request<Execution>('execution', {
      execution_id: id,
    });
    patch(tab.id, { session, execution });
    setResultIndex(0);
    setBottomTab('results');
    watchers.current.get(tab.id)?.();
    const dispose = await transport.watch(id, () => void refresh(tab.id, id));
    watchers.current.set(tab.id, dispose);
    await refresh(tab.id, id);
  };
  const closeOldResult = async (tab: Tab) => {
    if (tab.execution) {
      await transport.request('close_execution', {
        execution_id: tab.execution.id,
      });
      resultCache.clear(tab.id);
    }
  };
  const run = async (
    mode: 'current' | 'all' | 'explain' | 'commit' | 'rollback' = 'current',
  ) => {
    if (!active) {
      newTab();
      return;
    }
    if (running) return;
    setError('');
    setNotice('');
    try {
      let sql = active.sql;
      if (mode === 'commit' || mode === 'rollback') sql = mode.toUpperCase();
      else if (mode !== 'all') {
        const selection = editor.current?.selection();
        if (selection) sql = selection;
        else {
          const statements = await transport.request<
            { sql: string; start: number; end: number }[]
          >('prepare_script', { connection_id: active.connectionId, sql });
          const byte = new TextEncoder().encode(
            sql.slice(0, editor.current?.cursor() ?? 0),
          ).byteLength;
          sql =
            (
              statements.find((s) => byte >= s.start && byte < s.end) ??
              statements.at(-1)
            )?.sql ?? sql;
        }
        if (mode === 'explain') {
          const statements = await transport.request<{ sql: string }[]>(
            'prepare_script',
            { connection_id: active.connectionId, sql },
          );
          if (statements.length !== 1)
            throw new Error('Pilih satu statement untuk EXPLAIN.');
          sql = `EXPLAIN ${sql}`;
        }
      }
      const session = await ensureSession(active);
      await closeOldResult(active);
      const result = await transport.request<{ execution_id: string }>(
        'execute',
        { session_id: session.id, sql },
      );
      await attach(active, result.execution_id, session);
      if (mode === 'explain') setBottomTab('explain');
    } catch (e) {
      showError(e);
    }
  };
  const [recentFiles, setRecentFiles] = useState<
    { id: string; name: string }[]
  >([]);
  const fileAction = async (
    operation: 'open_file' | 'save_file' | 'open_recent',
    id?: string,
  ) => {
    if (!canExecute) return;
    if (!active || active.inspector) {
      setNotice('Buka tab SQL untuk memilih file.');
      return;
    }
    try {
      const file = await transport.request<{
        name: string;
        sql: string;
      } | null>(operation, { id, sql: active.sql });
      if (file && operation !== 'save_file')
        add({
          id: crypto.randomUUID(),
          connectionId: active.connectionId,
          title: file.name,
          sql: file.sql,
        });
      if (file && operation === 'save_file') {
        patch(active.id, { title: file.name });
        setNotice('File SQL disimpan di lokasi pilihan Anda.');
      }
      setRecentFiles(await transport.request('recent_files'));
    } catch (e) {
      showError(e);
    }
  };
  useEffect(() => {
    void transport
      .request<{ id: string; name: string }[]>('recent_files')
      .then(setRecentFiles)
      .catch(() => {});
  }, [transport]);
  const shortcutActions = useRef({
    save: () => {},
    search: () => {},
    format: () => {},
  });
  const [shortcutHelp, setShortcutHelp] = useState(false);
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        (event.ctrlKey || event.metaKey) &&
        event.altKey &&
        event.key.toLowerCase() === 'h'
      ) {
        event.preventDefault();
        setShortcutHelp((v) => !v);
        return;
      }
      if (!target?.closest('.monaco-editor')) return;
      if (
        (event.ctrlKey || event.metaKey) &&
        event.altKey &&
        event.key.toLowerCase() === 's'
      ) {
        event.preventDefault();
        shortcutActions.current.save();
        return;
      }
      if (
        (event.ctrlKey || event.metaKey) &&
        event.altKey &&
        event.key.toLowerCase() === 'j'
      ) {
        event.preventDefault();
        shortcutActions.current.search();
        return;
      }
      if (event.altKey && event.shiftKey && event.key.toLowerCase() === 'f') {
        event.preventDefault();
        shortcutActions.current.format();
        return;
      }
      if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
        event.preventDefault();
        void runRef.current(event.shiftKey ? 'all' : 'current');
      }
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, []);
  const connect = async () => {
    if (!selected) return;
    setError('');
    try {
      const session = await transport.request<Session>('connect', {
        connection_id: selected.id,
      });
      add({
        id: crypto.randomUUID(),
        connectionId: selected.id,
        title: 'Query baru',
        sql:
          selected.engine === 'MySql'
            ? 'SELECT VERSION();'
            : 'SELECT version();',
        session,
      });
    } catch (e) {
      showError(e);
    }
  };
  const closeTab = async (tab: Tab) => {
    if (!(await discardEditDrafts(transport, tab.id))) return;
    setError('');
    try {
      let current = tab;
      if (tab.execution && !terminal(tab.execution.status)) {
        if (
          !window.confirm(
            'Batalkan query dan tunggu cleanup sebelum menutup tab?',
          )
        )
          return;
        await transport.request('cancel', { execution_id: tab.execution.id });
        for (let i = 0; i < 100; i++) {
          const execution = await transport.request<Execution>('execution', {
            execution_id: tab.execution.id,
          });
          if (terminal(execution.status)) {
            current = { ...current, execution };
            break;
          }
          await new Promise((resolve) => setTimeout(resolve, 100));
        }
        if (current.execution && !terminal(current.execution.status))
          throw new Error(
            'Cleanup belum selesai. Coba tutup kembali setelah query berhenti.',
          );
      }
      if (tab.session) {
        const session = await transport.request<Session>('session', {
          session_id: tab.session.id,
        });
        const force = session.transaction !== 'Idle' || session.busy;
        if (
          force &&
          !window.confirm(
            'Session memiliki transaksi aktif atau tidak diketahui. Gunakan Commit/Rollback terlebih dahulu, atau OK untuk disconnect. SQL sebelumnya mungkin sudah tersimpan.',
          )
        )
          return;
        await transport.request('disconnect', {
          session_id: session.id,
          force,
        });
      }
      const p = profiles.data?.find((p) => p.id === tab.connectionId);
      closingTabs.current.add(tab.id);
      await saving.current.catch(() => {});
      if (p?.restore_sql && !tab.inspector) {
        await transport.request('delete_documents', {
          connection_id: tab.connectionId,
          kind: 'editor',
          id: tab.id,
        });
      }
      await import('./SqlEditor').then((module) =>
        module.disposeEditor(tab.id),
      );
      if (current.execution) await closeOldResult(current);
      watchers.current.get(tab.id)?.();
      watchers.current.delete(tab.id);
      remove(tab.id);
      savedText.current.delete(tab.id);
    } catch (e) {
      closingTabs.current.delete(tab.id);
      showError(e);
    }
  };
  const reconnect = async () => {
    if (!active) return;
    try {
      if (active.session) {
        if (
          !window.confirm(
            'Reconnect membuat session baru tanpa memulihkan transaksi, temporary table, atau menjalankan ulang SQL. Lanjutkan?',
          )
        )
          return;
        await transport.request('disconnect', {
          session_id: active.session.id,
          force: true,
        });
      }
      const session = await transport.request<Session>('connect', {
        connection_id: active.connectionId,
      });
      patch(active.id, { session });
      setNotice('Session baru terhubung.');
    } catch (e) {
      showError(e);
    }
  };
  const [documentOffset, setDocumentOffset] = useState(0);
  const [documentSearch, setDocumentSearch] = useState('');
  const [documentConnection, setDocumentConnection] = useState('');
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [searchFrom, setSearchFrom] = useState('');
  const [searchTo, setSearchTo] = useState('');
  const [searchStatus, setSearchStatus] = useState('');
  const [searchCursor, setSearchCursor] = useState<string | null>(null);
  const [searchComplete, setSearchComplete] = useState(true);
  const searchDocuments = async (
    kind: 'saved' | 'history' = documentKind,
    cursor: string | null = null,
  ) => {
    const id = documentConnection || profile?.id || selected?.id;
    if (!id) return;
    setDocumentConnection(id ?? '');
    setDocumentKind(kind);
    try {
      const value = await transport.request<{
        items: DocumentHeader[];
        next_cursor: string | null;
        complete: boolean;
      }>('search_documents', {
        connection_id: id,
        search: {
          kind,
          query: documentSearch,
          from_ms: searchFrom ? new Date(searchFrom).getTime() : null,
          to_ms: searchTo ? new Date(searchTo).getTime() : null,
          status: kind === 'history' && searchStatus ? searchStatus : null,
          favorites_only: kind === 'saved' && favoriteOnly,
          cursor,
        },
      });
      setDocuments(value.items);
      setSearchCursor(value.next_cursor);
      setSearchComplete(value.complete);
    } catch (e) {
      showError(e);
    }
  };
  const loadDocuments = async (kind: 'history' | 'saved', offset = 0) => {
    if (!profile && !selected) return;
    const id = documentConnection || profile?.id || selected?.id;
    setDocumentConnection(id ?? '');
    setDocumentKind(kind);
    try {
      setSearchCursor(null);
      setSearchComplete(true);
      setDocuments(
        await transport.request<DocumentHeader[]>('documents', {
          connection_id: id,
          kind,
          offset,
          limit: 100,
        }),
      );
      setDocumentOffset(offset);
    } catch (e) {
      showError(e);
    }
  };
  const saveQuery = async () => {
    if (!active) return;
    const name = window.prompt('Nama saved query', active.title);
    if (!name) return;
    try {
      const savedId = active.savedId ?? crypto.randomUUID();
      await transport.request('save_document', {
        document: {
          id: savedId,
          connection_id: active.connectionId,
          kind: 'saved',
          name,
          content: { sql: active.sql },
          created_at_ms: Date.now(),
        },
      });
      patch(active.id, { title: name, savedId });
      setNotice('Query tersimpan terenkripsi.');
    } catch (e) {
      showError(e);
    }
  };
  shortcutActions.current = {
    save: () => {
      if (canExecute) void saveQuery();
    },
    search: () => {
      if (canExecute) void loadDocuments('saved');
    },
    format: () => {
      if (canExecute) editor.current?.format();
    },
  };
  const openObject = (object: ObjectId) => {
    const id = connectionId;
    if (!id) return;
    const existing = tabs.find(
      (t) =>
        t.object?.name === object.name &&
        t.object.schema === object.schema &&
        t.connectionId === id &&
        t.object.kind === object.kind,
    );
    if (existing) {
      activate(existing.id);
      return;
    }
    add({
      id: crypto.randomUUID(),
      connectionId: id,
      title: object.name,
      sql: '',
      object,
      inspector: true,
    });
    setDocuments(null);
  };
  const resize = (axis: 'sidebar' | 'editor', event: React.PointerEvent) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    const start = axis === 'sidebar' ? event.clientX : event.clientY;
    const base = axis === 'sidebar' ? state.sidebarWidth : state.editorHeight;
    const element = event.currentTarget;
    const move = (e: Event) => {
      const pointer = e as PointerEvent;
      const value = Math.max(
        axis === 'sidebar' ? 180 : 150,
        Math.min(
          axis === 'sidebar' ? 500 : 650,
          base +
            (axis === 'sidebar' ? pointer.clientX : pointer.clientY) -
            start,
        ),
      );
      useWorkspace
        .getState()
        .layout(
          axis === 'sidebar'
            ? { sidebarWidth: value }
            : { editorHeight: value },
        );
    };
    const end = () => {
      element.removeEventListener('pointermove', move);
      element.removeEventListener('pointerup', end);
      const s = useWorkspace.getState();
      void transport
        .request('save_layout', {
          sidebar_width: s.sidebarWidth,
          editor_height: s.editorHeight,
        })
        .catch(showError);
    };
    element.addEventListener('pointermove', move);
    element.addEventListener('pointerup', end);
  };
  const execution = active?.execution;
  const result =
    execution?.results[
      Math.min(resultIndex, Math.max(0, execution.results.length - 1))
    ];
  return (
    <div className="workbench">
      <header className="toolbar">
        <div className="brand-mark" aria-hidden="true">
          ▥
        </div>
        <strong className="brand">Database Studio</strong>
        <span className="toolbar-divider" />
        <button disabled={!canManage} onClick={() => setDialog('new')}>
          <Icon name="plus" /> Koneksi
        </button>
        <button
          disabled={!connectionId || !canExecute}
          onClick={() => newTab()}
        >
          <Icon name="plus" /> SQL tab
        </button>
        <div className="toolbar-spacer" />
        <button
          disabled={!active || active.inspector}
          onClick={() => void saveQuery()}
        >
          Simpan query
        </button>
        <button
          disabled={!active || active.inspector}
          onClick={() => void fileAction('open_file')}
        >
          Open SQL
        </button>
        <button
          disabled={!active || active.inspector}
          onClick={() => void fileAction('save_file')}
        >
          Save file
        </button>
        {recentFiles.length ? (
          <select
            aria-label="Recent SQL files"
            value=""
            onChange={(e) => {
              if (e.target.value)
                void fileAction('open_recent', e.target.value);
            }}
          >
            <option value="">Recent files</option>
            {recentFiles.map((f) => (
              <option key={f.id} value={f.id}>
                {f.name}
              </option>
            ))}
          </select>
        ) : null}
        <button
          disabled={(!profile && !selected) || !canExecute}
          onClick={() => void loadDocuments('saved')}
        >
          Saved queries
        </button>
        <button
          disabled={(!profile && !selected) || !canExecute}
          onClick={() => void loadDocuments('history')}
        >
          History
        </button>
        <span className="local-badge">
          {isWeb ? 'Workspace web' : 'Desktop lokal'}
        </span>
      </header>
      {error || profiles.error ? (
        <div role="alert" className="banner error-banner">
          <span>{error || describeError(profiles.error)}</span>
          <button aria-label="Tutup pesan error" onClick={() => setError('')}>
            ×
          </button>
        </div>
      ) : null}
      {notice ? (
        <div role="status" className="banner notice-banner">
          {notice}
          <button
            aria-label="Tutup pemberitahuan"
            onClick={() => setNotice('')}
          >
            ×
          </button>
        </div>
      ) : null}
      <div className="workspace-body">
        <aside className="sidebar" style={{ width: state.sidebarWidth }}>
          <div className="sidebar-heading">
            <h2>Connections</h2>
            <button
              className="icon-button"
              aria-label="Refresh metadata"
              onClick={() => void query.invalidateQueries()}
            >
              <Icon name="refresh" />
            </button>
          </div>
          <div className="connection-list">
            {profiles.isPending ? (
              <p className="muted small">Memuat koneksi…</p>
            ) : profiles.data?.length ? (
              profiles.data.map((p) => (
                <button
                  key={p.id}
                  className={`connection-line${connectionId === p.id ? ' active-connection' : ''}`}
                  onClick={() => setConnectionId(p.id)}
                >
                  <span
                    className={`engine-glyph ${p.engine === 'MySql' ? 'mysql' : ''}`}
                  >
                    {p.engine === 'Postgres' ? 'P' : 'M'}
                  </span>
                  <span>
                    <strong>{p.name}</strong>
                    <small>
                      {p.host}:{p.port}
                    </small>
                  </span>
                  <span
                    className={`connection-dot${tabs.some((t) => t.connectionId === p.id && t.session?.valid) ? ' online' : ''}`}
                  />
                </button>
              ))
            ) : (
              <div className="connection-empty">
                <p>Belum ada koneksi.</p>
                <button disabled={!canManage} onClick={() => setDialog('new')}>
                  Tambah koneksi pertama
                </button>
              </div>
            )}
          </div>
          {selected ? (
            <>
              <div className="connection-actions">
                <button
                  disabled={connected || !canExecute}
                  onClick={() => void connect()}
                >
                  {!canExecute
                    ? 'Metadata'
                    : connected
                      ? 'Connected'
                      : 'Connect'}
                </button>
                <button
                  aria-label="Edit koneksi"
                  disabled={!canManage}
                  onClick={() => setDialog(selected)}
                >
                  Edit
                </button>
                <button
                  aria-label="Hapus koneksi"
                  disabled={!canManage}
                  onClick={() => {
                    if (
                      window.confirm(
                        `Hapus koneksi ${selected.name}? Semua session akan ditutup.`,
                      )
                    )
                      void transport
                        .request('delete_profile', {
                          connection_id: selected.id,
                          force: true,
                        })
                        .then(() => {
                          for (const tab of tabs.filter(
                            (t) => t.connectionId === selected.id,
                          )) {
                            resultCache.clear(tab.id);
                            watchers.current.get(tab.id)?.();
                            watchers.current.delete(tab.id);
                            void import('./SqlEditor').then((module) =>
                              module.disposeEditor(tab.id),
                            );
                            remove(tab.id);
                          }
                          setConnectionId(null);
                          void query.invalidateQueries({
                            queryKey: ['profiles'],
                          });
                        })
                        .catch(showError);
                  }}
                >
                  Hapus
                </button>
              </div>
              {connected ? (
                <Explorer
                  transport={transport}
                  profile={selected}
                  onOpen={openObject}
                  onNames={(names) =>
                    addNames(selected.id, selected.revision, names)
                  }
                />
              ) : (
                <p className="tree-note">
                  Connect untuk menelusuri object. Setiap tab SQL memiliki
                  session sendiri.
                </p>
              )}
            </>
          ) : null}
          <div className="sidebar-foot">
            PostgreSQL & MySQL
            <br />
            <span>Credential: OS atau memory-only</span>
          </div>
        </aside>
        <div
          className="resize-handle vertical"
          role="separator"
          aria-orientation="vertical"
          onPointerDown={(event) => resize('sidebar', event)}
        />
        <main className="main-panel">
          <nav className="tab-strip" aria-label="Tab workspace">
            {tabs.map((tab) => (
              <div
                key={tab.id}
                className={`workspace-tab${activeId === tab.id ? ' active-tab' : ''}`}
              >
                <button
                  onClick={() => {
                    activate(tab.id);
                    setConnectionId(tab.connectionId);
                    setDocuments(null);
                    editor.current = null;
                  }}
                >
                  <span className="tab-icon">
                    {tab.inspector ? <Icon name="table" /> : 'SQL'}
                  </span>
                  {tab.title}
                </button>
                <button
                  aria-label={`Tutup ${tab.title}`}
                  onClick={() => void closeTab(tab)}
                >
                  <Icon name="close" />
                </button>
              </div>
            ))}
            {tabs.length ? (
              <button
                className="new-tab"
                aria-label="Tab SQL baru"
                onClick={() => newTab()}
              >
                <Icon name="plus" />
              </button>
            ) : null}
          </nav>
          {documents ? (
            <section className="document-panel">
              <header>
                <h2>
                  {documentKind === 'history'
                    ? 'Query history'
                    : 'Saved queries'}
                </h2>
                <button onClick={() => setDocuments(null)}>
                  Kembali ke editor
                </button>
              </header>
              <div className="table-controls">
                <select
                  aria-label="Koneksi pencarian"
                  value={documentConnection}
                  onChange={(e) => {
                    setDocumentConnection(e.target.value);
                    setSearchCursor(null);
                    setDocuments([]);
                  }}
                >
                  {profiles.data?.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
                <input
                  aria-label="Cari nama atau SQL"
                  placeholder="Cari nama / SQL…"
                  value={documentSearch}
                  onChange={(e) => setDocumentSearch(e.target.value)}
                />
                {documentKind === 'saved' ? (
                  <label>
                    <input
                      type="checkbox"
                      checked={favoriteOnly}
                      onChange={(e) => setFavoriteOnly(e.target.checked)}
                    />
                    Favorit saya
                  </label>
                ) : null}
                <label>
                  Dari{' '}
                  <input
                    type="datetime-local"
                    aria-label="Cari dari tanggal"
                    value={searchFrom}
                    onChange={(e) => setSearchFrom(e.target.value)}
                  />
                </label>
                <label>
                  Sampai{' '}
                  <input
                    type="datetime-local"
                    aria-label="Cari sampai tanggal"
                    value={searchTo}
                    onChange={(e) => setSearchTo(e.target.value)}
                  />
                </label>
                {documentKind === 'history' ? (
                  <select
                    aria-label="Status history"
                    value={searchStatus}
                    onChange={(e) => setSearchStatus(e.target.value)}
                  >
                    <option value="">Semua status</option>
                    {['Completed', 'Failed', 'Cancelled'].map((status) => (
                      <option key={status}>{status}</option>
                    ))}
                  </select>
                ) : null}
                <button onClick={() => void searchDocuments()}>Cari</button>
                <button onClick={() => void loadDocuments(documentKind)}>
                  Reset
                </button>
                <button
                  disabled={!searchCursor}
                  onClick={() =>
                    void searchDocuments(documentKind, searchCursor)
                  }
                >
                  Lanjutkan pencarian
                </button>
                {!searchComplete ? (
                  <span>Pencarian belum lengkap; lanjutkan scan.</span>
                ) : null}
                <button
                  onClick={() =>
                    setDocuments((previous) =>
                      previous
                        ? [...previous].sort((a, b) =>
                            a.name.localeCompare(b.name),
                          )
                        : previous,
                    )
                  }
                >
                  Urutkan nama hasil halaman
                </button>
                <button
                  disabled={documentOffset === 0}
                  onClick={() =>
                    void loadDocuments(
                      documentKind,
                      Math.max(0, documentOffset - 100),
                    )
                  }
                >
                  Previous
                </button>
                <span>Halaman {Math.floor(documentOffset / 100) + 1}</span>
                <button
                  disabled={documents.length < 100}
                  onClick={() =>
                    void loadDocuments(documentKind, documentOffset + 100)
                  }
                >
                  Next
                </button>
              </div>
              {documents.length ? (
                documents.map((document) => (
                  <div key={document.id} className="document-line">
                    <button
                      onClick={() =>
                        void transport
                          .request<Document>('document', {
                            connection_id: document.connection_id,
                            kind: document.kind,
                            id: document.id,
                          })
                          .then((full) => {
                            add({
                              id: crypto.randomUUID(),
                              connectionId: full.connection_id,
                              title: full.name,
                              sql: full.content.sql,
                              savedId:
                                full.kind === 'saved' &&
                                document.can_edit !== false
                                  ? full.id
                                  : undefined,
                            });
                            setConnectionId(full.connection_id);
                            setDocuments(null);
                          })
                          .catch(showError)
                      }
                    >
                      <strong>{document.name}</strong>
                      <small>
                        {new Date(document.created_at_ms).toLocaleString(
                          'id-ID',
                        )}
                      </small>
                    </button>
                    {document.kind === 'saved' ? (
                      <button
                        aria-label={'Favorit ' + document.name}
                        onClick={() =>
                          void transport
                            .request('favorite', {
                              connection_id: document.connection_id,
                              id: document.id,
                              favorite: !document.favorite,
                            })
                            .then(() =>
                              setDocuments(
                                (previous) =>
                                  previous?.map((d) =>
                                    d.id === document.id
                                      ? { ...d, favorite: !d.favorite }
                                      : d,
                                  ) ?? null,
                              ),
                            )
                            .catch(showError)
                        }
                      >
                        {document.favorite ? '★' : '☆'}
                      </button>
                    ) : null}
                    <button
                      disabled={
                        document.can_edit === false ||
                        (isWeb && document.kind !== 'saved')
                      }
                      onClick={() => {
                        const name = window.prompt('Nama query', document.name);
                        if (name)
                          void transport
                            .request('rename_document', {
                              connection_id: document.connection_id,
                              kind: document.kind,
                              id: document.id,
                              name,
                            })
                            .then(() => loadDocuments(documentKind))
                            .catch(showError);
                      }}
                    >
                      Rename
                    </button>
                    <button
                      disabled={document.can_edit === false}
                      onClick={() =>
                        void transport
                          .request('delete_documents', {
                            connection_id: document.connection_id,
                            kind: document.kind,
                            id: document.id,
                          })
                          .then(() => loadDocuments(documentKind))
                          .catch(showError)
                      }
                    >
                      Hapus
                    </button>
                  </div>
                ))
              ) : (
                <p className="muted">
                  Belum ada query tersimpan. History perlu diaktifkan pada
                  pengaturan koneksi.
                </p>
              )}
            </section>
          ) : active && profiles.data && !profile ? (
            <section className="messages">
              <p role="alert">
                Koneksi tab ini telah dihapus. Pilih koneksi yang tersedia untuk
                membuka tab baru.
              </p>
              <button
                onClick={() => {
                  watchers.current.get(active.id)?.();
                  watchers.current.delete(active.id);
                  resultCache.clear(active.id);
                  void import('./SqlEditor').then((module) =>
                    module.disposeEditor(active.id),
                  );
                  remove(active.id);
                }}
              >
                Tutup tab
              </button>
            </section>
          ) : active?.inspector && active.object && profile ? (
            <Inspector
              tab={active}
              profile={profile}
              transport={transport}
              ensureSession={ensureSession}
              closeOldResult={closeOldResult}
              attach={attach}
              onError={showError}
              onNames={(names) =>
                addNames(profile!.id, profile!.revision, names)
              }
            />
          ) : active ? (
            <>
              <div className="editor-toolbar">
                <button
                  className="primary run-button"
                  disabled={Boolean(running)}
                  onClick={() => void run('current')}
                >
                  <Icon name="play" /> Run
                </button>
                <button
                  disabled={Boolean(running)}
                  onClick={() => void run('all')}
                >
                  Run script
                </button>
                <button
                  disabled={!running}
                  onClick={() =>
                    active.execution &&
                    void transport
                      .request('cancel', { execution_id: active.execution.id })
                      .catch(showError)
                  }
                >
                  <Icon name="stop" /> Cancel
                </button>
                <span className="toolbar-divider" />
                <button
                  disabled={Boolean(running)}
                  onClick={() => void run('explain')}
                >
                  Explain
                </button>
                <button
                  onClick={() => {
                    try {
                      editor.current?.format();
                    } catch (e) {
                      showError(e);
                    }
                  }}
                >
                  Format
                </button>
                <div className="toolbar-spacer" />
                <label className="commit-mode">
                  <input
                    type="checkbox"
                    checked={active.session?.auto_commit ?? true}
                    disabled={!active.session || Boolean(running)}
                    onChange={(e) => {
                      if (active.session)
                        void transport
                          .request<Session>('auto_commit', {
                            session_id: active.session.id,
                            enabled: e.target.checked,
                          })
                          .then((session) => patch(active.id, { session }))
                          .catch(showError);
                    }}
                  />
                  Auto Commit
                </label>
                <button
                  disabled={!active.session || Boolean(running)}
                  onClick={() => void run('commit')}
                >
                  Commit
                </button>
                <button
                  disabled={!active.session || Boolean(running)}
                  onClick={() => void run('rollback')}
                >
                  Rollback
                </button>
              </div>
              <div className="editor-context">
                <span>
                  {profile?.name} <span className="muted">/</span>{' '}
                  {profile?.database}
                </span>
                <span>
                  {active.session
                    ? `Session ${active.session.id.slice(0, 8)} · ${active.session.transaction}`
                    : 'Session dibuka saat query pertama'}
                </span>
              </div>
              <div
                className="editor-container"
                style={{ height: state.editorHeight }}
              >
                <Suspense
                  fallback={
                    <p className="editor-loading">Memuat SQL editor…</p>
                  }
                >
                  <SqlEditor
                    key={active.id}
                    tab={active}
                    engine={profile?.engine ?? 'Postgres'}
                    onChange={(sql) => patch(active.id, { sql })}
                    onReady={(handle) => {
                      editor.current = handle;
                    }}
                    completions={
                      completionScope.current ===
                        profile!.id + '/' + profile!.revision &&
                      Date.now() - completionUpdated.current < 300000
                        ? completions
                        : []
                    }
                  />
                </Suspense>
              </div>
              <div
                className="resize-handle horizontal"
                role="separator"
                aria-orientation="horizontal"
                onPointerDown={(event) => resize('editor', event)}
              />
              <div className="result-tabs">
                <button
                  className={bottomTab === 'results' ? 'selected' : ''}
                  onClick={() => setBottomTab('results')}
                >
                  Results
                  {execution?.results.length
                    ? ` (${execution.results.length})`
                    : ''}
                </button>
                <button
                  className={bottomTab === 'messages' ? 'selected' : ''}
                  onClick={() => setBottomTab('messages')}
                >
                  Messages
                </button>
                <button
                  className={bottomTab === 'explain' ? 'selected' : ''}
                  onClick={() => setBottomTab('explain')}
                >
                  Explain
                </button>
                <div className="toolbar-spacer" />
                <span
                  className={`execution-status ${execution?.status.toLowerCase() ?? ''}`}
                >
                  {execution?.status ?? 'Ready'}
                </span>
                {execution && terminal(execution.status) ? (
                  <button
                    onClick={() =>
                      void closeOldResult(active)
                        .then(() => patch(active.id, { execution: undefined }))
                        .catch(showError)
                    }
                  >
                    Tutup hasil
                  </button>
                ) : null}
              </div>
              {execution?.results.length && bottomTab !== 'messages' ? (
                <div className="statement-tabs">
                  {execution.results.map((r, i) => (
                    <button
                      key={r.id}
                      className={resultIndex === i ? 'selected' : ''}
                      onClick={() => setResultIndex(i)}
                    >
                      Statement {r.statement + 1}
                    </button>
                  ))}
                </div>
              ) : null}
              {bottomTab === 'messages' ? (
                <div className="messages">
                  <p>{execution?.status ?? 'Belum ada eksekusi.'}</p>
                  {execution?.error ? (
                    <p className="error-text">{execution.error.message}</p>
                  ) : null}
                  {execution?.persistence_error ? (
                    <p className="error-text">
                      History tidak tersimpan:{' '}
                      {execution.persistence_error.message}
                    </p>
                  ) : null}
                  {execution?.results.map((r) => (
                    <div key={r.id}>
                      <strong>Statement {r.statement + 1}</strong>
                      <pre>{JSON.stringify(r.terminal, null, 2)}</pre>
                    </div>
                  ))}
                </div>
              ) : execution && result ? (
                <ResultGrid
                  key={result.id}
                  transport={transport}
                  execution={execution}
                  result={result}
                  owner={active.id}
                  onTransform={(updated) =>
                    patch(active.id, { execution: updated })
                  }
                />
              ) : (
                <div className="result-empty">
                  <Icon name="table" />
                  <strong>Hasil query akan tampil di sini</strong>
                  <p>
                    Jalankan statement dengan Ctrl / ⌘ Enter, atau seluruh
                    script dengan Shift.
                  </p>
                  {execution?.error ? (
                    <p className="error-text">{execution.error.message}</p>
                  ) : null}
                </div>
              )}
            </>
          ) : (
            <section className="welcome">
              <div className="welcome-glyph">
                <Icon name="database" />
              </div>

              <h1>
                Dari schema
                <br />
                ke jawaban.
              </h1>
              <p>
                Telusuri PostgreSQL dan MySQL, tulis SQL, dan periksa hasilnya
                dalam satu workspace {isWeb ? 'web' : 'lokal'}.
              </p>
              <button
                className="primary"
                disabled={!canManage}
                onClick={() => setDialog('new')}
              >
                <Icon name="plus" /> Tambah koneksi
              </button>
              <div className="welcome-details">
                <div>
                  <strong>Session per editor</strong>
                  <span>
                    Transaksi dan temporary table tetap pada tab yang sama.
                  </span>
                </div>
                <div>
                  <strong>Hasil bertahap</strong>
                  <span>Grid virtual dan penyimpanan hasil yang dibatasi.</span>
                </div>
                <div>
                  <strong>Penyimpanan aman</strong>
                  <span>
                    {isWeb
                      ? 'Credential dikelola backend. History SQL pribadi dan opt-in.'
                      : 'Password di credential store OS. History SQL bersifat opt-in.'}
                  </span>
                </div>
              </div>
            </section>
          )}
        </main>
      </div>
      <footer className="statusbar">
        <span>
          <span
            className={`connection-dot${active?.session?.valid ? ' online' : ''}`}
          />
          {active?.session?.valid
            ? `${profile?.name} · ${profile?.engine}`
            : 'Tidak ada session aktif'}
        </span>
        <span>
          {active?.session
            ? `Transaksi: ${active.session.transaction}`
            : 'Pilih koneksi untuk mulai'}
          {active?.session &&
          profile &&
          active.session.revision !== profile.revision
            ? ' · Revisi lama; reconnect untuk menerapkan perubahan'
            : ''}
        </span>
        <button disabled={!active} onClick={() => void reconnect()}>
          Reconnect
        </button>
        <span className="toolbar-spacer" />
        <span>UTF-8</span>
        <span>SQL</span>
      </footer>
      <button
        className="shortcut-help"
        onClick={() => setShortcutHelp((v) => !v)}
      >
        Shortcuts
      </button>
      {shortcutHelp ? (
        <section role="dialog" aria-label="Keyboard shortcuts">
          <button onClick={() => setShortcutHelp(false)}>Tutup bantuan</button>
          <p>
            Di editor: Ctrl/⌘ Enter menjalankan statement; Shift menjalankan
            script. Ctrl/⌘ Alt S menyimpan query, Ctrl/⌘ Alt J membuka pencarian
            saved queries, Alt Shift F memformat. Tombol menu tersedia jika
            kombinasi ditangani browser/OS. Ctrl/⌘ Alt H membuka bantuan.
          </p>
        </section>
      ) : null}
      {dialog ? (
        <ConnectionDialog
          initial={dialog === 'new' ? undefined : dialog}
          transport={transport}
          onClose={() => setDialog(null)}
          onSaved={() =>
            void query.invalidateQueries({ queryKey: ['profiles'] })
          }
        />
      ) : null}
    </div>
  );
}

function Inspector({
  tab,
  profile,
  transport,
  ensureSession,
  closeOldResult,
  attach,
  onError,
  onNames,
}: {
  tab: Tab;
  profile: Profile;
  transport: DatabaseTransport;
  ensureSession: (tab: Tab) => Promise<Session>;
  closeOldResult: (tab: Tab) => Promise<void>;
  attach: (tab: Tab, id: string, session: Session) => Promise<void>;
  onError: (e: unknown) => void;
  onNames: (names: string[]) => void;
}) {
  const [section, setSection] = useState('Columns');
  const [offset, setOffset] = useState(0);
  const [limit, setLimit] = useState(100);
  const [sort, setSort] = useState('');
  const [descending, setDescending] = useState(false);
  const [column, setColumn] = useState('');
  const [filter, setFilter] = useState('');
  const metadata = useQuery({
    queryKey: ['inspect', profile.id, profile.revision, tab.object],
    queryFn: async () => {
      const value = await transport.request<Metadata>('metadata', {
        connection_id: profile.id,
        request: { Inspect: { object: tab.object } },
      });
      onNames(
        value.columns.flatMap((c) => [
          tab.object!.name + '.' + c.name,
          ...(tab.object!.schema
            ? [tab.object!.schema + '.' + tab.object!.name + '.' + c.name]
            : []),
        ]),
      );
      return value;
    },
  });
  const queryable =
    (transport.capabilities?.execute ?? true) &&
    ['Table', 'View', 'MaterializedView'].includes(tab.object!.kind);
  const data = async (next = offset, sortColumn = sort, desc = descending) => {
    try {
      const current =
        useWorkspace.getState().tabs.find((t) => t.id === tab.id) ?? tab;
      if (current.execution && !terminal(current.execution.status))
        throw new Error('Tunggu eksekusi saat ini selesai.');
      const session = await ensureSession(current);
      await closeOldResult(current);
      const result = await transport.request<{ execution_id: string }>(
        'table',
        {
          session_id: session.id,
          object: tab.object,
          offset: next,
          limit,
          sort_column: sortColumn,
          descending: desc,
          filter_column: column,
          filter,
        },
      );
      await attach(current, result.execution_id, session);
      setOffset(next);
    } catch (e) {
      onError(e);
    }
  };
  return (
    <div className="inspector">
      <header>
        <div>
          <span className="muted small">
            {profile.database}
            {tab.object?.schema ? ` / ${tab.object.schema}` : ''} /{' '}
            {tab.object?.kind}
          </span>
          <h1>{tab.object?.name}</h1>
        </div>
        <button onClick={() => void metadata.refetch()}>
          <Icon name="refresh" /> Refresh
        </button>
      </header>
      <nav className="inspector-tabs">
        {[
          'Data',
          'Columns',
          'Indexes',
          'Constraints',
          'Foreign Keys',
          'DDL',
          ...((transport.capabilities?.execute ?? true) &&
          metadata.data?.editing?.transactional &&
          metadata.data.editing.primary_key.length
            ? ['Edit']
            : []),
        ].map((name) => (
          <button
            key={name}
            disabled={name === 'Data' && !queryable}
            className={section === name ? 'selected' : ''}
            onClick={() => {
              setSection(name);
              if (name === 'Data' && !tab.execution) void data();
            }}
          >
            {name}
          </button>
        ))}
      </nav>
      {metadata.error ? (
        <p role="alert" className="error-text">
          {describeError(metadata.error)}
        </p>
      ) : metadata.isPending ? (
        <p className="muted">Memuat definisi…</p>
      ) : section === 'Data' ? (
        <>
          <div className="table-controls">
            <label>
              Rows
              <select
                value={limit}
                onChange={(e) => setLimit(Number(e.target.value))}
              >
                {[100, 250, 500, 1000].map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </select>
            </label>
            <select
              aria-label="Kolom filter"
              value={column}
              onChange={(e) => setColumn(e.target.value)}
            >
              <option value="">Pilih kolom filter</option>
              {metadata.data.columns.map((c) => (
                <option key={c.name}>{c.name}</option>
              ))}
            </select>
            <input
              aria-label="Filter berisi"
              placeholder="Nilai berisi…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <button onClick={() => void data(0)}>Terapkan / Refresh</button>
            <div className="toolbar-spacer" />
            <button
              disabled={offset === 0}
              onClick={() => void data(Math.max(0, offset - limit))}
            >
              Previous
            </button>
            <span>
              {offset + 1}–{offset + (tab.execution?.results[0]?.rows ?? 0)}
            </span>
            <button onClick={() => void data(offset + limit)}>Next</button>
          </div>
          <p className="table-caveat small">
            Halaman bukan snapshot. Tanpa key unik, rows dapat berubah
            antarhalaman. Klik header untuk sorting di database.
          </p>
          {tab.execution?.results[0] ? (
            <ResultGrid
              transport={transport}
              execution={tab.execution}
              result={tab.execution.results[0]}
              owner={tab.id}
              onSort={(name) => {
                const desc = sort === name ? !descending : false;
                setSort(name);
                setDescending(desc);
                void data(0, name, desc);
              }}
            />
          ) : (
            <p className="muted">Memuat data…</p>
          )}
          {tab.execution?.error ? (
            <p className="error-text">{tab.execution.error.message}</p>
          ) : null}
        </>
      ) : section === 'Edit' ? (
        <TableEditor
          transport={transport}
          connection={profile.id}
          object={tab.object!}
          tabId={tab.id}
        />
      ) : section === 'DDL' ? (
        <DefinitionText
          transport={transport}
          text={
            metadata.data.ddl ?? 'Definisi tidak tersedia untuk object ini.'
          }
        />
      ) : (
        <div className="definition-scroll">
          <table className="definition-table">
            <thead>
              <tr>
                {section === 'Columns' ? (
                  <>
                    <th>Column</th>
                    <th>Type</th>
                    <th>Nullable</th>
                    <th>Default</th>
                  </>
                ) : section === 'Indexes' ? (
                  <>
                    <th>Index</th>
                    <th>Unique</th>
                    <th>Definition</th>
                  </>
                ) : (
                  <>
                    <th>Constraint</th>
                    <th>Kind</th>
                    <th>Definition</th>
                  </>
                )}
              </tr>
            </thead>
            <tbody>
              {section === 'Columns'
                ? metadata.data.columns.map((c) => (
                    <tr key={c.name}>
                      <td>{c.name}</td>
                      <td>{c.data_type}</td>
                      <td>{c.nullable ? 'Yes' : 'No'}</td>
                      <td>{c.default ?? '—'}</td>
                    </tr>
                  ))
                : section === 'Indexes'
                  ? metadata.data.indexes.map((i) => (
                      <tr key={i.name}>
                        <td>{i.name}</td>
                        <td>{i.unique ? 'Yes' : 'No'}</td>
                        <td>{i.definition}</td>
                      </tr>
                    ))
                  : metadata.data.constraints
                      .filter(
                        (c) =>
                          section !== 'Foreign Keys' ||
                          c.kind === 'FOREIGN KEY',
                      )
                      .map((c) => (
                        <tr key={c.name}>
                          <td>{c.name}</td>
                          <td>{c.kind}</td>
                          <td>{c.definition}</td>
                        </tr>
                      ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
