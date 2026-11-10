import { readFileSync, readdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function procPids() {
  return readdirSync('/proc')
    .filter((pid) => /^\d+$/.test(pid))
    .map(Number);
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
      !baseline.has(pid) &&
      commandLine(pid).includes('database-studio-desktop'),
  );
}

export async function runNativePhase({
  pnpm,
  args,
  env,
  label,
  idleGateFile,
  idleSampleCompleteFile,
}) {
  try {
    unlinkSync(idleGateFile);
  } catch {}
  try {
    unlinkSync(idleSampleCompleteFile);
  } catch {}
  const baseline =
    process.platform === 'linux' ? new Set(procPids()) : undefined;
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
      'NATIVE_PROCESS_METRICS',
      JSON.stringify({
        phase: label,
        platform: process.platform,
        startup: {
          available: false,
          limitation:
            'startup unavailable: WDIO launches the native binary, so no real process-launch timestamp is observable',
        },
        whole_process_idle_rss_kb: null,
        instrumentation:
          'missing: process/RSS instrumentation is intentionally Linux-only',
      }),
    );
    return exitCode;
  }
  let idleGateSeen;
  let idleRssKb;
  const deadline = Date.now() + 120000;
  while (!idleGateSeen && Date.now() < deadline && child.exitCode === null) {
    const apps = appPids(baseline);
    if (apps.length && !idleGateSeen) {
      try {
        readFileSync(idleGateFile, 'utf8');
        idleGateSeen = true;
      } catch {}
    }
    await sleep(100);
  }
  if (idleGateSeen) {
    const roots = appPids(baseline);
    const pids = new Set(roots.flatMap((pid) => processTree(pid)));
    if (pids.size) {
      idleRssKb = [...pids].reduce((total, pid) => total + rssKb(pid), 0);
      if (idleRssKb > 0)
        writeFileSync(idleSampleCompleteFile, `${Date.now()}\n`);
    }
  }
  const exitCode = await exit;
  console.log(
    'NATIVE_PROCESS_METRICS',
    JSON.stringify({
      phase: label,
      platform: process.platform,
      startup: {
        available: false,
        limitation:
          'startup unavailable: no process-launch timestamp from the native launcher',
      },
      whole_process_idle_rss_kb: idleRssKb ?? null,
      instrumentation:
        idleRssKb > 0
          ? 'Linux /proc process tree sampled after the explicit pre-work idle gate and before database connections'
          : 'missing: native process or positive idle RSS was not observable',
    }),
  );
  return exitCode || (idleRssKb > 0 ? 0 : 1);
}
