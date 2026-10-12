import { Icon } from './Icon';
import { useState } from 'react';
import type { DatabaseTransport, Profile } from '@database-studio/contracts';
import { describeError } from '@database-studio/contracts';
export function newProfile(): Profile {
  return {
    id: crypto.randomUUID(),
    revision: 0,
    name: '',
    engine: 'Postgres',
    host: 'localhost',
    port: 5432,
    username: '',
    database: '',
    ssl: 'verify_full',
    connect_timeout_ms: 10000,
    query_timeout_ms: null,
    history_enabled: false,
    restore_sql: false,
  };
}
export function ConnectionDialog({
  initial,
  transport,
  onClose,
  onSaved,
}: {
  initial?: Profile;
  transport: DatabaseTransport;
  onClose: () => void;
  onSaved: () => void;
}) {
  const isWeb = transport.capabilities?.platform === 'web';
  const [profile, setProfile] = useState<Profile>(
    initial ?? { ...newProfile(), ...(isWeb ? { host: '127.0.0.1' } : {}) },
  );
  const [password, setPassword] = useState('');
  const [remember, setRemember] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const update = <K extends keyof Profile>(key: K, value: Profile[K]) =>
    setProfile((p) => ({ ...p, [key]: value }));
  const perform = async (save: boolean) => {
    if (
      save &&
      isWeb &&
      ((initial?.history_enabled && !profile.history_enabled) ||
        (initial?.restore_sql && !profile.restore_sql)) &&
      !window.confirm(
        'Menonaktifkan history/restore akan menghapus dokumen terkait untuk semua anggota koneksi ini. Lanjutkan?',
      )
    )
      return;
    setBusy(true);
    setMessage('');
    try {
      await transport.request(save ? 'save_profile' : 'test', {
        profile,
        password: password || (initial ? undefined : ''),
        remember,
      });
      if (save) {
        setPassword('');
        if (
          !isWeb &&
          initial?.history_enabled &&
          !profile.history_enabled &&
          window.confirm(
            'History dinonaktifkan. Hapus history lama untuk koneksi ini?',
          )
        )
          await transport.request('delete_documents', {
            connection_id: profile.id,
            kind: 'history',
          });
        onSaved();
        onClose();
      } else setMessage('Koneksi berhasil.');
    } catch (e) {
      setMessage(describeError(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      className="overlay"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <section
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="connection-title"
      >
        <header>
          <div>
            <h2 id="connection-title">
              {initial ? 'Edit koneksi' : 'Koneksi baru'}
            </h2>
            <p>
              {isWeb
                ? 'Hubungkan database yang diizinkan operator.'
                : 'Hubungkan database dari komputer Anda.'}
            </p>
          </div>
          <button
            className="icon-button"
            aria-label="Tutup dialog"
            onClick={onClose}
          >
            <Icon name="close" />
          </button>
        </header>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void perform(true);
          }}
        >
          <label>
            Nama
            <input
              required
              autoFocus
              value={profile.name}
              onChange={(e) => update('name', e.target.value)}
              placeholder="Contoh: Analytics lokal"
            />
          </label>
          <div className="field-pair">
            <label>
              Database engine
              <select
                value={profile.engine}
                onChange={(e) => {
                  const engine = e.target.value as Profile['engine'];
                  setProfile((p) => ({
                    ...p,
                    engine,
                    port: engine === 'Postgres' ? 5432 : 3306,
                  }));
                }}
              >
                <option>Postgres</option>
                <option>MySql</option>
              </select>
            </label>
            <label>
              Database
              <input
                required
                value={profile.database}
                onChange={(e) => update('database', e.target.value)}
              />
            </label>
          </div>
          <div className="field-host">
            <label>
              Host
              <input
                required
                value={profile.host}
                onChange={(e) => update('host', e.target.value)}
              />
            </label>
            <label>
              Port
              <input
                required
                type="number"
                min="1"
                max="65535"
                value={profile.port || ''}
                onChange={(e) => update('port', Number(e.target.value))}
              />
            </label>
          </div>
          <div className="field-pair">
            <label>
              Username
              <input
                required
                autoComplete="off"
                value={profile.username}
                onChange={(e) => update('username', e.target.value)}
              />
            </label>
            <label>
              Password
              <input
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder={
                  initial ? 'Kosongkan untuk credential tersimpan' : ''
                }
              />
            </label>
          </div>
          <label>
            TLS
            <select
              value={profile.ssl}
              onChange={(e) => update('ssl', e.target.value)}
            >
              <option value="verify_full">Verify certificate & hostname</option>
              <option value="verify_ca">Verify certificate</option>
              <option value="require">Require TLS (tanpa verifikasi)</option>
              <option value="prefer">Prefer TLS (boleh plaintext)</option>
              <option value="disable">Disabled (koneksi plaintext)</option>
            </select>
          </label>
          <div className="field-pair">
            <label>
              Connect timeout (ms)
              <input
                type="number"
                min="1"
                max="120000"
                value={profile.connect_timeout_ms}
                onChange={(e) =>
                  update('connect_timeout_ms', Number(e.target.value))
                }
              />
            </label>
            <label>
              Query timeout (ms, opsional)
              <input
                type="number"
                min="1"
                value={profile.query_timeout_ms ?? ''}
                onChange={(e) =>
                  update(
                    'query_timeout_ms',
                    e.target.value ? Number(e.target.value) : null,
                  )
                }
              />
            </label>
          </div>
          {!isWeb && (
            <label className="check">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              Simpan password di credential store OS
            </label>
          )}
          {isWeb && <p>Credential disimpan terenkripsi oleh backend.</p>}
          <label className="check">
            <input
              type="checkbox"
              checked={profile.history_enabled}
              onChange={(e) => update('history_enabled', e.target.checked)}
            />
            Simpan history SQL terenkripsi (30 hari / 1.000 eksekusi)
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={profile.restore_sql}
              onChange={(e) => update('restore_sql', e.target.checked)}
            />
            Pulihkan teks editor saat aplikasi dibuka kembali
          </label>
          {message ? (
            <p role="status" className="dialog-message">
              {message}
            </p>
          ) : null}
          <footer>
            <button
              type="button"
              disabled={busy}
              onClick={() => void perform(false)}
            >
              Test koneksi
            </button>
            <button type="submit" className="primary" disabled={busy}>
              {busy ? 'Memproses…' : 'Simpan koneksi'}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
