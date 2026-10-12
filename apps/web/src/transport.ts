import type {
  DatabaseTransport,
  Notification,
  Document,
  Profile,
  Session,
  Row,
} from '@database-studio/contracts';
export interface Envelope<T> {
  data: T;
  message: string;
  meta: { pagination?: { total: number; total_pages: number } };
}
export class ApiFailure extends Error {
  constructor(
    message: string,
    public status: number,
    public code: string,
  ) {
    super(message);
  }
}
export async function api<T>(
  path: string,
  csrf: string,
  method = 'GET',
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<Envelope<T>> {
  const response = await fetch(path, {
    method,
    credentials: 'same-origin',
    cache: 'no-store',
    headers: {
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      ...(method === 'GET' ? {} : { 'X-CSRF-Token': csrf }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const value = await response
    .json()
    .catch(() => ({ message: 'Respons server tidak dapat dibaca.' }));
  if (!response.ok)
    throw new ApiFailure(
      value.message ?? 'Operasi gagal.',
      response.status,
      value.details?.[0]?.code ?? 'HTTP_ERROR',
    );
  return value;
}
export class HttpDatabaseTransport implements DatabaseTransport {
  readonly capabilities;
  private tabToken = btoa(
    String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32))),
  )
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '');
  private sessions = new Set<string>();
  private sessionInfo = new Map<string, Session>();
  private revisions = new Map<string, number>();
  private watchers = new Map<string, (event: Notification) => void>();
  private socket: WebSocket | null = null;
  private ready: Promise<void> | null = null;
  private pageTail: Promise<unknown> = Promise.resolve();
  private queuedPages = 0;
  private pendingPages = new Map<
    string,
    { resolve: (rows: Row[]) => void; reject: (error: Error) => void }
  >();
  private metadataTail = new Map<string, Promise<unknown>>();
  private metadataPending = new Map<string, number>();
  private closeHandler: (() => Promise<boolean>) | undefined;
  private closed = false;
  private heartbeat: number;
  private reconnectTimer: number | undefined;
  constructor(
    readonly base: string,
    private csrf: string,
    role: string,
    private onAuthLost: () => void,
    private onAccessLost: () => void = onAuthLost,
  ) {
    this.capabilities = {
      platform: 'web' as const,
      manageConnections: ['Owner', 'Admin'].includes(role),
      execute: role !== 'Viewer',
      persistentCredentials: true,
    };
    this.heartbeat = window.setInterval(() => {
      void this.http<{ role: string }>('')
        .then((scope) => {
          if (scope.role !== role) this.onAccessLost();
        })
        .catch(() => {});
      for (const id of this.sessions)
        void this.http(`/sessions/${id}/lease`, 'POST').catch((error) => {
          if (
            error instanceof ApiFailure &&
            [401, 403, 404, 410].includes(error.status)
          )
            this.sessions.delete(id);
        });
    }, 10000);
  }
  private async http<T>(
    path: string,
    method = 'GET',
    body?: unknown,
    headers?: Record<string, string>,
  ): Promise<T> {
    if (this.closed) throw new Error('Workspace ditutup.');
    try {
      const value = await api<T>(this.base + path, this.csrf, method, body, {
        ...headers,
        'X-Studio-Tab': this.tabToken,
      });
      if (this.closed) throw new Error('Workspace ditutup.');
      return value.data;
    } catch (error) {
      if (error instanceof ApiFailure && error.status === 401)
        this.onAuthLost();
      if (error instanceof ApiFailure && error.status === 403)
        this.onAccessLost();
      throw error;
    }
  }
  async onCloseRequested(handler: () => Promise<boolean>): Promise<() => void> {
    this.closeHandler = handler;
    return () => {
      this.closeHandler = undefined;
    };
  }
  async closeWorkspace(): Promise<boolean> {
    if (this.closed) return true;
    if (this.closeHandler && !(await this.closeHandler())) return false;
    for (const id of this.sessions)
      await this.http(`/sessions/${id}?force=true`, 'DELETE').catch((error) => {
        if (
          !(
            error instanceof ApiFailure &&
            [401, 403, 404, 410].includes(error.status)
          )
        )
          throw error;
      });
    this.dispose();
    return true;
  }
  dispose() {
    this.closed = true;
    this.sessions.clear();
    this.sessionInfo.clear();
    this.revisions.clear();
    window.clearInterval(this.heartbeat);
    window.clearTimeout(this.reconnectTimer);
    this.socket?.close();
    this.socket = null;
    this.ready = null;
    this.watchers.clear();
    for (const pending of this.pendingPages.values())
      pending.reject(new Error('Workspace ditutup.'));
    this.pendingPages.clear();
  }
  private async ensureSocket(): Promise<void> {
    if (this.closed) throw new Error('Workspace ditutup.');
    if (this.ready) return this.ready;
    this.ready = new Promise<void>((resolve, reject) => {
      const url = new URL(this.base + '/stream', location.href);
      url.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const socket = new WebSocket(url, 'studio.v1');
      this.socket = socket;
      let started = false;
      const timer = window.setTimeout(() => {
        reject(new Error('Stream tidak merespons.'));
        socket.close();
      }, 10000);
      socket.onopen = () =>
        socket.send(
          JSON.stringify({
            type: 'hello',
            version: 1,
            csrf: this.csrf,
            tab_token: this.tabToken,
          }),
        );
      socket.onmessage = (event) => {
        if (
          typeof event.data !== 'string' ||
          new TextEncoder().encode(event.data).byteLength > 2 * 1024 * 1024
        ) {
          socket.close();
          return;
        }
        let value: Record<string, unknown>;
        try {
          value = JSON.parse(event.data);
        } catch {
          socket.close();
          return;
        }
        if (value.type === 'ready') {
          started = true;
          window.clearTimeout(timer);
          for (const id of this.watchers.keys())
            socket.send(JSON.stringify({ type: 'watch', execution_id: id }));
          resolve();
          return;
        }
        if (typeof value.sequence === 'number')
          socket.send(
            JSON.stringify({ type: 'ack', sequence: value.sequence }),
          );
        if (value.type === 'notification')
          this.watchers.get(String(value.execution_id))?.(
            value as unknown as Notification,
          );
        if (value.type === 'page' || value.type === 'page_error') {
          const id = String(value.request_id);
          const pending = this.pendingPages.get(id);
          this.pendingPages.delete(id);
          if (value.type === 'page') pending?.resolve(value.rows as Row[]);
          else
            pending?.reject(
              new Error(
                String(value.message ?? 'Halaman hasil tidak tersedia.'),
              ),
            );
        }
      };
      socket.onerror = () => {
        if (!started) reject(new Error('Stream tidak dapat terhubung.'));
      };
      socket.onclose = () => {
        window.clearTimeout(timer);
        if (!started) reject(new Error('Stream terputus.'));
        this.ready = null;
        this.socket = null;
        for (const pending of this.pendingPages.values())
          pending.reject(
            new Error(
              'Stream terputus; status query dapat diperiksa tanpa menjalankan ulang SQL.',
            ),
          );
        this.pendingPages.clear();
        if (!this.closed && this.watchers.size)
          this.reconnectTimer = window.setTimeout(
            () => void this.ensureSocket().catch(() => {}),
            2000,
          );
      };
    });
    return this.ready;
  }
  async watch(
    id: string,
    onEvent: (event: Notification) => void,
  ): Promise<() => void> {
    this.watchers.set(id, onEvent);
    await this.ensureSocket();
    this.socket?.send(JSON.stringify({ type: 'watch', execution_id: id }));
    return () => {
      this.watchers.delete(id);
      if (this.socket?.readyState === WebSocket.OPEN)
        this.socket.send(JSON.stringify({ type: 'unwatch', execution_id: id }));
    };
  }
  private page(args: Record<string, unknown>): Promise<Row[]> {
    if (this.queuedPages >= 32)
      return Promise.reject(new Error('Antrean halaman penuh.'));
    this.queuedPages++;
    const job = this.pageTail
      .catch(() => {})
      .then(async () => {
        await this.ensureSocket();
        const id = crypto.randomUUID();
        return new Promise<Row[]>((resolve, reject) => {
          const timer = window.setTimeout(() => {
            this.pendingPages.delete(id);
            reject(new Error('Pengambilan halaman melewati batas waktu.'));
          }, 35000);
          this.pendingPages.set(id, {
            resolve: (rows) => {
              window.clearTimeout(timer);
              resolve(rows);
            },
            reject: (error) => {
              window.clearTimeout(timer);
              reject(error);
            },
          });
          this.socket?.send(
            JSON.stringify({ type: 'page', request_id: id, ...args }),
          );
        });
      });
    this.pageTail = job;
    return job.finally(() => {
      this.queuedPages--;
    });
  }
  async request<T>(
    operation: string,
    args: Record<string, unknown> = {},
  ): Promise<T> {
    const conn = `/connections/${String(args.connection_id)}`;
    const session = `/sessions/${String(args.session_id)}`;
    const execution = `/executions/${String(args.execution_id)}`;
    let result: unknown;
    switch (operation) {
      case 'handshake': {
        const scope = await this.http<{ version: number }>('');
        if (scope.version !== 1 || args.version !== 1)
          throw new Error('Versi transport tidak kompatibel.');
        result = { version: 1 };
        break;
      }
      case 'profiles':
        result = await this.http('/connections?per_page=100');
        break;
      case 'save_profile': {
        const profile = args.profile as Profile;
        result = await this.http(
          profile.revision ? `/connections/${profile.id}` : '/connections',
          profile.revision ? 'PUT' : 'POST',
          { profile, password: args.password ?? null },
        );
        break;
      }
      case 'test':
        result = await this.http('/connections/test', 'POST', {
          profile: args.profile,
          password: args.password ?? null,
        });
        break;
      case 'delete_profile':
        result = await this.http(conn, 'DELETE');
        break;
      case 'connect': {
        const value = await this.http<Session>(conn + '/sessions', 'POST');
        this.sessions.add(value.id);
        this.sessionInfo.set(value.id, value);
        result = value;
        break;
      }
      case 'session': {
        try {
          const value = await this.http<Session>(session);
          this.sessionInfo.set(value.id, value);
          result = value;
        } catch (error) {
          const previous = this.sessionInfo.get(String(args.session_id));
          if (
            previous &&
            error instanceof ApiFailure &&
            [404, 410].includes(error.status)
          ) {
            this.sessions.delete(previous.id);
            result = {
              ...previous,
              valid: false,
              busy: false,
              transaction: 'Unknown',
            };
          } else throw error;
        }
        break;
      }
      case 'disconnect': {
        try {
          result = await this.http(
            session + `?force=${args.force === true}`,
            'DELETE',
          );
        } catch (error) {
          if (
            !(error instanceof ApiFailure && [404, 410].includes(error.status))
          )
            throw error;
          result = null;
        }
        this.sessions.delete(String(args.session_id));
        this.sessionInfo.delete(String(args.session_id));
        break;
      }
      case 'auto_commit':
        result = await this.http(session + '/auto-commit', 'PUT', {
          enabled: args.enabled,
        });
        break;
      case 'metadata': {
        const id = String(args.connection_id),
          pending = this.metadataPending.get(id) ?? 0;
        if (pending >= 16) throw new Error('Tunggu pemuatan metadata selesai.');
        this.metadataPending.set(id, pending + 1);
        const job = (this.metadataTail.get(id) ?? Promise.resolve())
          .catch(() => {})
          .then(() => this.http(conn + '/metadata', 'POST', args.request));
        this.metadataTail.set(id, job);
        try {
          result = await job;
        } finally {
          this.metadataPending.set(id, (this.metadataPending.get(id) ?? 1) - 1);
          if (this.metadataTail.get(id) === job) {
            this.metadataTail.delete(id);
            this.metadataPending.delete(id);
          }
        }
        break;
      }
      case 'prepare_script':
        result = await this.http(conn + '/prepare', 'POST', { sql: args.sql });
        break;
      case 'execute':
      case 'table': {
        const key = String(args.idempotency_key ?? crypto.randomUUID());
        try {
          result = await this.http(
            session + (operation === 'execute' ? '/executions' : '/table'),
            'POST',
            operation === 'execute' ? { sql: args.sql } : args,
            { 'Idempotency-Key': key },
          );
        } catch (error) {
          if (error instanceof ApiFailure && error.status < 500) throw error;
          // A metadata GET may recover a lost admission response. Never retry SQL POST.
          try {
            result = await this.http(
              `/executions?idempotency_key=${encodeURIComponent(key)}`,
            );
          } catch {
            throw new Error(
              'Status eksekusi belum diketahui. SQL tidak dijalankan ulang. Periksa session sebelum menjalankan SQL lagi.',
            );
          }
        }
        break;
      }
      case 'execution':
        result = await this.http(execution);
        break;
      case 'cancel':
        result = await this.http(execution + '/cancel', 'POST');
        break;
      case 'close_execution':
        result = await this.http(execution, 'DELETE');
        break;
      case 'page':
        result = await this.page(args);
        break;
      case 'transform_result':
        result = await this.http(
          execution + `/results/${String(args.result_id)}/transform`,
          'POST',
          {
            column: args.column,
            descending: args.descending,
            filter: args.filter,
          },
        );
        break;
      case 'edit_snapshot':
        result = await this.http(conn + '/edits/snapshot', 'POST', {
          object: args.object,
          tab_id: args.tab_id,
          offset: args.offset ?? 0,
        });
        break;
      case 'edit_readback':
        result = await this.http(conn + '/edits/readback', 'POST', {
          tab_id: args.tab_id,
          token: args.token,
        });
        break;
      case 'edit_preview':
        result = await this.http(conn + '/edits/preview', 'POST', args.batch);
        break;
      case 'edit_discard':
        result = await this.http(
          conn + '/edits/' + String(args.tab_id),
          'DELETE',
        );
        break;
      case 'edit_apply': {
        const key = String(args.idempotency_key);
        try {
          result = await this.http(conn + '/mutations', 'POST', args.batch, {
            'Idempotency-Key': key,
          });
        } catch (error) {
          if (error instanceof ApiFailure && error.status < 500) throw error;
          result = await this.http(
            conn + '/mutations?idempotency_key=' + encodeURIComponent(key),
          ).catch(() => {
            throw new Error(
              'Status mutation tidak diketahui. Periksa data; Apply tidak diulang otomatis.',
            );
          });
        }
        break;
      }
      case 'edit_status':
        result = await this.http(
          conn + '/mutations/' + String(args.mutation_id),
        );
        break;
      case 'edit_cancel':
        result = await this.http(
          conn + '/mutations/' + String(args.mutation_id),
          'DELETE',
        );
        break;
      case 'search_documents': {
        const value = await this.http<{
          items: (Document & { revision: number })[];
        }>(conn + '/documents/search', 'POST', args.search);
        for (const item of value.items)
          this.revisions.set(item.id, item.revision);
        result = value;
        break;
      }
      case 'favorite': {
        result = await this.http(
          conn + '/documents/' + String(args.id) + '/favorite',
          'PUT',
          { favorite: args.favorite },
        );
        break;
      }
      case 'documents': {
        const page = Math.floor(Number(args.offset ?? 0) / 100) + 1;
        const value = await this.http<(Document & { revision: number })[]>(
          conn +
            `/documents?kind=${String(args.kind ?? 'saved')}&page=${page}&per_page=100`,
        );
        for (const document of value)
          this.revisions.set(document.id, document.revision);
        result = value;
        break;
      }
      case 'document': {
        const value = await this.http<Document & { revision: number }>(
          conn +
            `/documents/${String(args.id)}?kind=${String(args.kind ?? 'saved')}`,
        );
        this.revisions.set(value.id, value.revision);
        result = value;
        break;
      }
      case 'save_document': {
        const document = args.document as Document;
        const saved = await this.http<{ revision: number }>(
          `/connections/${document.connection_id}/documents`,
          'POST',
          { ...document, revision: this.revisions.get(document.id) ?? 0 },
        );
        this.revisions.set(document.id, saved.revision);
        result = null;
        break;
      }
      case 'rename_document': {
        const id = String(args.id);
        const saved = await this.http<{ revision: number }>(
          conn + `/documents/${id}?kind=${String(args.kind ?? 'saved')}`,
          'PATCH',
          { name: args.name, revision: this.revisions.get(id) ?? 0 },
        );
        this.revisions.set(id, saved.revision);
        result = null;
        break;
      }
      case 'delete_documents': {
        const revision = args.id
          ? this.revisions.get(String(args.id))
          : undefined;
        if (args.id && args.kind === 'editor' && revision === undefined) {
          result = null;
          break;
        }
        const suffix = revision === undefined ? '' : `&revision=${revision}`;
        result = await this.http(
          conn +
            `/documents${args.id ? '/' + String(args.id) : ''}?kind=${String(args.kind ?? 'saved')}${suffix}`,
          'DELETE',
        );
        if (args.id) this.revisions.delete(String(args.id));
        break;
      }
      case 'layout':
        result = await this.http('/layout');
        break;
      case 'save_layout':
        result = await this.http('/layout', 'PUT', args);
        break;
      case 'recent_files':
        result = [];
        break;
      case 'copy_text':
        await navigator.clipboard.writeText(String(args.text ?? ''));
        result = null;
        break;
      case 'open_file':
        result = await openSql();
        break;
      case 'save_file': {
        const sql = String(args.sql ?? '');
        if (new TextEncoder().encode(sql).byteLength > 1024 * 1024)
          throw new Error('SQL dibatasi 1 MiB.');
        const name = window.prompt('Nama file SQL', 'query.sql');
        if (!name) {
          result = null;
          break;
        }
        download(
          new Blob([sql], { type: 'text/plain;charset=utf-8' }),
          name.replaceAll(/[\\/]/g, '_'),
        );
        result = { name, sql };
        break;
      }
      default:
        throw new Error('Operasi tidak tersedia pada platform web.');
    }
    return result as T;
  }
}
function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
function openSql(): Promise<{ name: string; sql: string } | null> {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.sql,text/plain';
    input.oncancel = () => resolve(null);
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      if (file.size > 1024 * 1024) {
        reject(new Error('SQL dibatasi 1 MiB.'));
        return;
      }
      void file
        .arrayBuffer()
        .then((buffer) =>
          resolve({
            name: file.name,
            sql: new TextDecoder('utf-8', { fatal: true }).decode(buffer),
          }),
        )
        .catch(() => reject(new Error('Gunakan file SQL UTF-8 yang valid.')));
    };
    input.click();
  });
}
