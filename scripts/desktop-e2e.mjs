import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { runNativePhase } from './native-benchmark.mjs';
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
  if (built) {
    process.exitCode = built;
  } else {
    const readyFile = path.join(data, 'native-ready');
    const wdioArgs = ['exec', 'wdio', 'run', 'wdio.conf.mjs'];
    const first = await runNativePhase({
      pnpm,
      args: wdioArgs,
      env: {
        ...env,
        STUDIO_E2E_PERSIST_PROFILE: '1',
        STUDIO_NATIVE_BENCHMARK_READY_FILE: readyFile,
      },
      label: 'initial',
      readyFile,
    });
    process.exitCode = first;
    if (!first) {
      process.exitCode = await runNativePhase({
        pnpm,
        args: wdioArgs,
        env: {
          ...env,
          STUDIO_E2E_RESTART_CHECK: '1',
          STUDIO_NATIVE_BENCHMARK_READY_FILE: readyFile,
        },
        label: 'restart',
        readyFile,
      });
    }
  }
} finally {
  rmSync(data, { recursive: true, force: true });
}
