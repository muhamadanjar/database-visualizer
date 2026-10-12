import { afterEach, expect, it, vi } from 'vitest';
import { HttpDatabaseTransport, api, ApiFailure } from './transport';
const envelope = (data: unknown, status = 200) =>
  new Response(JSON.stringify({ message: 'OK', data, meta: {} }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it('does not retry customer execution after a lost HTTP response', async () => {
  const fetch = vi.fn().mockRejectedValue(new TypeError('connection lost'));
  vi.stubGlobal('fetch', fetch);
  const transport = new HttpDatabaseTransport(
    '/api/v1/organizations/o/workspaces/w',
    'fixture-csrf',
    'Developer',
    () => {},
  );
  try {
    await expect(
      transport.request('execute', {
        session_id: 'fixture-session',
        sql: 'SELECT 1',
        idempotency_key: 'same-intent',
      }),
    ).rejects.toThrow('Status eksekusi belum diketahui');
    expect(
      fetch.mock.calls.filter((call) => call[1].method === 'POST'),
    ).toHaveLength(1);
    expect(fetch.mock.calls[1][1].method).toBe('GET');
    expect(fetch.mock.calls[0][1].headers['Idempotency-Key']).toBe(
      'same-intent',
    );
  } finally {
    transport.dispose();
  }
});

it('checks the server transport version and refuses requests after workspace disposal', async () => {
  const fetch = vi.fn().mockResolvedValue(envelope({ version: 2 }));
  vi.stubGlobal('fetch', fetch);
  const transport = new HttpDatabaseTransport(
    '/api/v1/organizations/o/workspaces/w',
    'fixture-csrf',
    'Viewer',
    () => {},
  );
  expect(transport.capabilities.execute).toBe(false);
  expect(transport.capabilities.manageConnections).toBe(false);
  await expect(transport.request('handshake', { version: 1 })).rejects.toThrow(
    'Versi transport',
  );
  transport.dispose();
  await expect(transport.request('profiles')).rejects.toThrow(
    'Workspace ditutup',
  );
  expect(fetch).toHaveBeenCalledTimes(1);
});

it('keeps authentication in same-origin cookies and clears browser workspace access on revocation', async () => {
  const fetch = vi.fn().mockResolvedValue(
    new Response(
      JSON.stringify({
        message: 'Permission denied',
        details: [{ code: 'PERMISSION_DENIED' }],
      }),
      { status: 403, headers: { 'Content-Type': 'application/json' } },
    ),
  );
  vi.stubGlobal('fetch', fetch);
  const authLost = vi.fn(),
    accessLost = vi.fn();
  const transport = new HttpDatabaseTransport(
    '/api/v1/organizations/o/workspaces/w',
    'fixture-csrf',
    'Developer',
    authLost,
    accessLost,
  );
  try {
    await expect(
      transport.request('execute', {
        session_id: 'fixture-session',
        sql: 'SELECT 1',
      }),
    ).rejects.toBeInstanceOf(ApiFailure);
    expect(accessLost).toHaveBeenCalledOnce();
    expect(authLost).not.toHaveBeenCalled();
    const request = fetch.mock.calls[0][1];
    expect(request.credentials).toBe('same-origin');
    expect(request.cache).toBe('no-store');
    expect(request.headers['X-CSRF-Token']).toBe('fixture-csrf');
    expect(request.headers.Authorization).toBeUndefined();
  } finally {
    transport.dispose();
  }
  await expect(api('/api/v1/auth/me', '')).rejects.toBeInstanceOf(ApiFailure);
});

it('does not deliver a private document response after switching out of its workspace', async () => {
  let complete!: (value: Response) => void;
  vi.stubGlobal(
    'fetch',
    vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          complete = resolve;
        }),
    ),
  );
  const transport = new HttpDatabaseTransport(
    '/api/v1/organizations/a/workspaces/old',
    'fixture-csrf',
    'Developer',
    () => {},
  );
  const document = transport.request('document', {
    connection_id: 'fixture-connection',
    id: 'fixture-document',
    kind: 'saved',
  });
  transport.dispose();
  complete(
    envelope({
      id: 'fixture-document',
      revision: 1,
      content: { sql: 'SELECT private_fixture' },
    }),
  );
  await expect(document).rejects.toThrow('Workspace ditutup');
});

it('looks up mutation status after response loss and never reposts Apply', async () => {
  const fetch = vi
    .fn()
    .mockRejectedValueOnce(new TypeError('lost'))
    .mockResolvedValue(envelope({ id: 'm', state: 'Unknown' }));
  vi.stubGlobal('fetch', fetch);
  const transport = new HttpDatabaseTransport(
    '/workspace',
    'csrf',
    'Developer',
    () => {},
  );
  const result = await transport.request('edit_apply', {
    connection_id: 'c',
    idempotency_key: 'k',
    batch: { token: 't', changes: [] },
  });
  expect(result).toEqual({ id: 'm', state: 'Unknown' });
  expect(fetch.mock.calls[0][1].method).toBe('POST');
  expect(fetch.mock.calls[1][1].method).toBe('GET');
  expect(fetch.mock.calls[1][0]).toContain('idempotency_key=k');
  transport.dispose();
});
