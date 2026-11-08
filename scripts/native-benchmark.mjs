import { readFileSync, readdirSync, unlinkSync } from 'node:fs';
import { spawn } from 'node:child_process';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function procPids() {
  return readdirSync('/proc', { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && /^\d+$/.test(entry.name))
    .map((entry) => Number(entry.name));
}

function commandLine(pid) {
  try {
    return readFileSync(`/proc/${pid}/cmdline`, 'utf8').replaceAll('\0', ' ');
  } catch {
    return '';
  }
}

function rssKb(pid) {
  try {
    const status = readFileSync(`/proc/${pid}/status`, 'utf8');
    return Number(status.match(/^VmRSS:\s+(\d+) kB$/m)?.[1] ?? 0);
  } catch {
    return 0;
  }
}

function children(pid) {
  try {
    return readFileSync(`/proc/${pid}/task/${pid}/children`, 'utf8')
      .trim()
      .split(/\s+/)
      .filter(Boolean)
      .map(Number);
  } catch {
    return [];
  }
}

function processTree(root) {
  const seen = new Set();
  const visit = (pid) => {
    if (seen.has(pid)) return;
    seen.add(pid);
    for (const child of children(pid)) visit(child);
  };
  visit(root);
  return [...seen];
}

function appPids(baseline) {
  return procPids().filter(
    (pid) =>
      !baseline.has(pid) && commandLine(pid).includes('database-studio-desktop'),
  );
}

export async function runNativePhase({ pnpm, args, env, label, readyFile }) {
  try {
    unlinkSync(readyFile);
  } catch {}
  const baseline = new Set(procPids());
  const child = spawn(pnpm, args, {
    env,
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  const exit = new Promise((resolve) => {
    child.once('exit', (code, signal) => resolve(code ?? (signal ? 1 : 0)));
  });
  if (process.platform !== 'linux') {
    const exitCode = await exit;
    console.log(
      'NATIVE_STARTUP',
      JSON.stringify({
        phase: label,
        platform: process.platform,
        cold_startup_ms: null,
        whole_process_idle_rss_kb: null,
        instrumentation:
          'missing: process/RSS instrumentation is intentionally Linux-only',
      }),
    );
    return exitCode;
  }
  let appSeenWall;
  let readyWall;
  let idleRssKb;
  const deadline = Date.now() + 120000;
  while (!readyWall && Date.now() < deadline && child.exitCode === null) {
    const apps = appPids(baseline);
    if (apps.length && !appSeenWall) appSeenWall = Date.now();
    if (apps.length && !readyWall) {
      try {
        readyWall = Number(readFileSync(readyFile, 'utf8').trim());
      } catch {}
    }
    await sleep(100);
  }
  if (readyWall && appSeenWall) {
    await sleep(2000);
    const roots = appPids(baseline);
    const pids = new Set(roots.flatMap((pid) => processTree(pid)));
    idleRssKb = [...pids].reduce((total, pid) => total + rssKb(pid), 0);
  }
  const exitCode = await exit;
  console.log(
    'NATIVE_STARTUP',
    JSON.stringify({
      phase: label,
      platform: process.platform,
      cold_startup_ms:
        readyWall && appSeenWall ? readyWall - appSeenWall : null,
      whole_process_idle_rss_kb: idleRssKb ?? null,
      instrumentation:
        readyWall && appSeenWall
          ? 'Linux /proc process tree and native ready marker'
          : 'missing: native process or ready marker was not observable',
    }),
  );
  return exitCode;
}
