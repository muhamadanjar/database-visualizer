import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
const data = mkdtempSync(path.join(tmpdir(), 'database-studio-native-'));
const env = {
  ...process.env,
  DATABASE_STUDIO_TEST_DATA: data,
  VITE_DESKTOP_E2E: '1',
};
const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
function run(args) {
  const result = spawnSync(pnpm, args, {
    env,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.error) throw result.error;
  return result.status ?? 1;
}
try {
  const built = run([
    '--filter',
    '@database-studio/desktop',
    'tauri',
    'build',
    '--debug',
    '--features',
    'desktop-e2e',
    '--config',
    'src-tauri/tauri.e2e.conf.json',
    '--no-bundle',
  ]);
  process.exitCode = built || run(['exec', 'wdio', 'run', 'wdio.conf.mjs']);
} finally {
  rmSync(data, { recursive: true, force: true });
}
