import React, { useEffect, useState, lazy, Suspense } from 'react';
import { createRoot } from 'react-dom/client';
import { clearWorkspace } from '@database-studio/workspace/lifecycle';
import { api, ApiFailure, HttpDatabaseTransport } from './transport';
import '@database-studio/workspace/styles.css';
import './web.css';
import { Administration } from './Administration';
const Workspace = lazy(() =>
  import('@database-studio/workspace').then((module) => ({
    default: module.Workspace,
  })),
);
interface Identity {
  user_id: string;
  display_name: string;
  csrf_token: string;
}
interface Organization {
  id: string;
  name: string;
  role: string;
}
interface WorkspaceInfo {
  id: string;
  name: string;
  role: string;
}
function App() {
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [loading, setLoading] = useState(true);
  const [providers, setProviders] = useState<string[]>([]);
  const [organizations, setOrganizations] = useState<Organization[]>([]);
  const [organization, setOrganization] = useState<Organization | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [current, setCurrent] = useState<{
    workspace: WorkspaceInfo;
    transport: HttpDatabaseTransport;
  } | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [administration, setAdministration] = useState(false);
  const fail = (error: unknown) =>
    setError(error instanceof Error ? error.message : 'Operasi gagal.');
  const authLost = () => {
    setIdentity(null);
    setError('Session berakhir. Login kembali; SQL tidak dijalankan ulang.');
  };
  const load = async () => {
    try {
      const me = await api<Identity>('/api/v1/auth/me', '');
      setIdentity(me.data);
      setOrganizations(
        (
          await api<Organization[]>(
            '/api/v1/organizations?per_page=100',
            me.data.csrf_token,
          )
        ).data,
      );
    } catch {
      setIdentity(null);
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void load();
    void api<string[]>('/api/v1/auth/providers', '')
      .then((value) => setProviders(value.data))
      .catch(fail);
  }, []);
  useEffect(() => {
    if (!identity) {
      current?.transport.dispose();
      void clearWorkspace();
      setCurrent(null);
      setOrganization(null);
      setWorkspaces([]);
    }
  }, [identity]);
  useEffect(() => {
    if (!identity) return;
    let touched = 0;
    const activity = (event: Event) => {
      if (!event.isTrusted || Date.now() - touched < 60000) return;
      touched = Date.now();
      void api('/api/v1/auth/activity', identity.csrf_token, 'POST').catch(
        (error) => {
          if (error instanceof ApiFailure && error.status === 401) authLost();
        },
      );
    };
    document.addEventListener('pointerdown', activity);
    document.addEventListener('keydown', activity);
    return () => {
      document.removeEventListener('pointerdown', activity);
      document.removeEventListener('keydown', activity);
    };
  }, [identity]);
  const leave = async () => {
    if (current && !(await current.transport.closeWorkspace())) return false;
    await clearWorkspace();
    setAdministration(false);
    setCurrent(null);
    return true;
  };
  const selectOrganization = async (org: Organization) => {
    try {
      if (!(await leave())) return;
      setOrganization(org);
      setWorkspaces(
        (
          await api<WorkspaceInfo[]>(
            `/api/v1/organizations/${org.id}/workspaces?per_page=100`,
            identity!.csrf_token,
          )
        ).data,
      );
    } catch (error) {
      fail(error);
    }
  };
  const openWorkspace = async (workspace: WorkspaceInfo) => {
    try {
      if (!(await leave())) return;
      setCurrent({
        workspace,
        transport: new HttpDatabaseTransport(
          `/api/v1/organizations/${organization!.id}/workspaces/${workspace.id}`,
          identity!.csrf_token,
          workspace.role,
          authLost,
          () => {
            setCurrent((value) => {
              value?.transport.dispose();
              return null;
            });
            void clearWorkspace();
            setError(
              'Akses workspace berubah. Refresh akses sebelum membuka kembali.',
            );
          },
        ),
      });
    } catch (error) {
      fail(error);
    }
  };
  const createWorkspace = async () => {
    const name = window.prompt('Nama workspace');
    if (!name || !organization || !identity) return;
    setBusy(true);
    try {
      await api(
        `/api/v1/organizations/${organization.id}/workspaces`,
        identity.csrf_token,
        'POST',
        { name },
      );
      setWorkspaces(
        (
          await api<WorkspaceInfo[]>(
            `/api/v1/organizations/${organization.id}/workspaces?per_page=100`,
            identity.csrf_token,
          )
        ).data,
      );
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  const logout = async () => {
    setBusy(true);
    try {
      if (!(await leave())) return;
      await api('/api/v1/auth/logout', identity!.csrf_token, 'POST');
      setIdentity(null);
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  if (loading)
    return (
      <main className="web-login">
        <p role="status">Memuat session…</p>
      </main>
    );
  if (!identity)
    return (
      <main className="web-login">
        <section>
          <span className="web-wordmark">Database Studio</span>
          <h1>
            Workspace database
            <br />
            untuk tim Anda.
          </h1>
          <p>
            Login untuk membuka workspace yang telah diberikan kepada akun Anda.
          </p>
          {error && <p role="alert">{error}</p>}
          <div className="web-login-actions">
            {providers
              .filter((provider) => provider !== 'test')
              .map((provider) => (
                <a
                  className="primary web-login-link"
                  key={provider}
                  href={`/api/v1/auth/${provider}/login`}
                >
                  Login dengan {provider === 'github' ? 'GitHub' : 'Google'}
                </a>
              ))}
          </div>
          {providers.includes('test') &&
            import.meta.env.VITE_TEST_PROVIDER === '1' && (
              <button
                onClick={() => {
                  const subject = window.prompt('Fixture account', 'owner');
                  if (subject)
                    void api('/api/v1/auth/test/login', '', 'POST', { subject })
                      .then(load)
                      .catch(fail);
                }}
              >
                Login fixture lokal
              </button>
            )}
          {!providers.length && (
            <p>Provider login belum dikonfigurasi oleh operator.</p>
          )}
        </section>
      </main>
    );
  return (
    <div className="web-app">
      <header className="web-account">
        <strong>Database Studio</strong>
        <label>
          Organisasi{' '}
          <select
            aria-label="Organisasi"
            value={organization?.id ?? ''}
            onChange={(event) => {
              const selected = organizations.find(
                (org) => org.id === event.target.value,
              );
              if (selected) void selectOrganization(selected);
            }}
          >
            <option value="">Pilih organisasi</option>
            {organizations.map((org) => (
              <option key={org.id} value={org.id}>
                {org.name}
              </option>
            ))}
          </select>
        </label>
        {organization && (
          <label>
            Workspace{' '}
            <select
              aria-label="Workspace"
              value={current?.workspace.id ?? ''}
              onChange={(event) => {
                const selected = workspaces.find(
                  (workspace) => workspace.id === event.target.value,
                );
                if (selected) void openWorkspace(selected);
              }}
            >
              <option value="">Pilih workspace</option>
              {workspaces.map((workspace) => (
                <option key={workspace.id} value={workspace.id}>
                  {workspace.name} · {workspace.role}
                </option>
              ))}
            </select>
          </label>
        )}
        {organization?.role === 'Owner' && (
          <button disabled={busy} onClick={() => void createWorkspace()}>
            Workspace baru
          </button>
        )}
        {current?.transport.capabilities.manageConnections && (
          <button onClick={() => setAdministration(true)}>
            Kelola workspace
          </button>
        )}
        <span
          className="web-account-name"
          title={`ID akun: ${identity.user_id}`}
        >
          {identity.display_name}
        </span>
        <button
          onClick={() => void navigator.clipboard.writeText(identity.user_id)}
        >
          Copy ID akun
        </button>
        <button disabled={busy} onClick={() => void logout()}>
          Logout
        </button>
      </header>
      {error && (
        <div role="alert" className="banner error-banner">
          <span>{error}</span>
          <button onClick={() => setError('')}>Tutup</button>
        </div>
      )}
      {administration && current && organization && (
        <Administration
          base={current.transport.base}
          organization={organization.id}
          owner={organization.role === 'Owner'}
          csrf={identity.csrf_token}
          onClose={() => setAdministration(false)}
          onWorkspaceChanged={async (deleted) => {
            const fresh = (
              await api<WorkspaceInfo[]>(
                `/api/v1/organizations/${organization.id}/workspaces?per_page=100`,
                identity.csrf_token,
              )
            ).data;
            setWorkspaces(fresh);
            if (deleted) {
              current.transport.dispose();
              await clearWorkspace();
              setCurrent(null);
              setAdministration(false);
            } else {
              const updated = fresh.find(
                (value) => value.id === current.workspace.id,
              );
              if (updated) setCurrent({ ...current, workspace: updated });
            }
          }}
        />
      )}
      {current ? (
        <Suspense
          fallback={<main className="web-login">Memuat workspace…</main>}
        >
          <Workspace key={current.workspace.id} transport={current.transport} />
        </Suspense>
      ) : (
        <main className="web-login">
          <section>
            <h1>
              {organizations.length
                ? 'Pilih workspace.'
                : 'Akses belum diberikan.'}
            </h1>
            <p>
              {organizations.length
                ? 'Pilih organisasi lalu workspace di atas untuk mulai bekerja.'
                : 'Login berhasil. Berikan ID akun kepada Owner/Admin untuk mendapatkan akses.'}
            </p>
            <code>{identity.user_id}</code>
            <button onClick={() => void load()}>Refresh akses</button>
          </section>
        </main>
      )}
    </div>
  );
}
createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
