import { useEffect, useState } from 'react';
import { api } from './transport';
interface Member {
  user_id: string;
  display_name: string;
  role: string;
}
export function Administration({
  base,
  organization,
  owner,
  csrf,
  onClose,
  onWorkspaceChanged,
}: {
  base: string;
  organization: string;
  owner: boolean;
  csrf: string;
  onClose: () => void;
  onWorkspaceChanged: (deleted?: boolean) => Promise<void>;
}) {
  const [members, setMembers] = useState<Member[]>([]),
    [error, setError] = useState(''),
    [busy, setBusy] = useState(false);
  const [user, setUser] = useState(''),
    [role, setRole] = useState('Viewer'),
    [page, setPage] = useState(1),
    [pages, setPages] = useState(1);
  const [scope, setScope] = useState<'workspace' | 'organization'>('workspace');
  const memberBase =
    scope === 'organization' ? `/api/v1/organizations/${organization}` : base;
  const fail = (error: unknown) =>
    setError(error instanceof Error ? error.message : 'Operasi gagal.');
  const refresh = async () => {
    try {
      const response = await api<Member[]>(
        `${memberBase}/members?page=${page}&per_page=20`,
        csrf,
      );
      setMembers(response.data);
      setPages(Math.max(1, response.meta.pagination?.total_pages ?? 1));
    } catch (error) {
      fail(error);
    }
  };
  useEffect(() => {
    void refresh();
  }, [memberBase, page]);
  const submit = async () => {
    if (!user.trim()) return;
    setBusy(true);
    setError('');
    try {
      if (owner)
        await api(
          `/api/v1/organizations/${organization}/members`,
          csrf,
          'POST',
          { user_id: user, role: 'Member' },
        );
      if (scope === 'workspace')
        await api(`${base}/members`, csrf, 'POST', { user_id: user, role });
      setUser('');
      await refresh();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  const remove = async (member: Member) => {
    if (
      !window.confirm(
        `Cabut akses ${member.display_name} dari ${scope === 'organization' ? 'organisasi dan seluruh workspace' : 'workspace'} ini?`,
      )
    )
      return;
    setBusy(true);
    try {
      await api(`${memberBase}/members/${member.user_id}`, csrf, 'DELETE');
      await refresh();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  const rename = async () => {
    const name = window.prompt('Nama workspace baru');
    if (!name) return;
    setBusy(true);
    try {
      await api(base, csrf, 'PATCH', { name });
      await onWorkspaceChanged();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  const deleteWorkspace = async () => {
    if (
      !window.confirm(
        'Hapus workspace beserta koneksi dan dokumennya? Session aktif akan ditutup.',
      )
    )
      return;
    setBusy(true);
    try {
      await api(base, csrf, 'DELETE');
      await onWorkspaceChanged(true);
      onClose();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="overlay">
      <section
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="administration-title"
      >
        <header>
          <h2 id="administration-title">Kelola workspace</h2>
          <button aria-label="Tutup pengelolaan" onClick={onClose}>
            Tutup
          </button>
        </header>
        {error && <p role="alert">{error}</p>}
        <label>
          Cakupan anggota{' '}
          <select
            value={scope}
            onChange={(event) => {
              setScope(event.target.value as typeof scope);
              setPage(1);
            }}
          >
            <option value="workspace">Workspace</option>
            {owner && <option value="organization">Organisasi</option>}
          </select>
        </label>
        <p className="small">
          Pengguna harus sudah login dan memberikan ID akunnya. Admin dapat
          memberikan akses workspace kepada anggota organisasi yang sudah ada.
        </p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label>
            ID akun
            <input
              required
              value={user}
              onChange={(event) => setUser(event.target.value)}
            />
          </label>
          {scope === 'workspace' && (
            <label>
              Role
              <select
                value={role}
                onChange={(event) => setRole(event.target.value)}
              >
                {['Admin', 'Developer', 'Viewer'].map((value) => (
                  <option key={value}>{value}</option>
                ))}
              </select>
            </label>
          )}
          <button disabled={busy} type="submit">
            Tambahkan / perbarui akses
          </button>
        </form>
        <ul>
          {members.map((member) => (
            <li key={member.user_id}>
              <strong>{member.display_name}</strong> · {member.role}
              <br />
              <code>{member.user_id}</code>{' '}
              <button
                disabled={busy || member.role === 'Owner'}
                onClick={() => void remove(member)}
              >
                Cabut akses
              </button>
            </li>
          ))}
        </ul>
        <div>
          <button disabled={page <= 1} onClick={() => setPage(page - 1)}>
            Sebelumnya
          </button>{' '}
          {page} / {pages}{' '}
          <button disabled={page >= pages} onClick={() => setPage(page + 1)}>
            Berikutnya
          </button>
        </div>
        <footer>
          <button disabled={busy} onClick={() => void rename()}>
            Rename workspace
          </button>
          <button disabled={busy} onClick={() => void deleteWorkspace()}>
            Hapus workspace
          </button>
        </footer>
      </section>
    </div>
  );
}
