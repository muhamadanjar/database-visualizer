import { spawnSync } from 'node:child_process';
import {
  mkdirSync,
  readFileSync,
  writeFileSync,
  mkdtempSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const session = 'database-studio-web-e2e';
mkdirSync('test-results/web', { recursive: true });
const source = readFileSync('tests/web/workspace.browser.js', 'utf8')
  .trim()
  .replace(/;$/, '');
new Function(`return (${source})`); // Syntax check only; browser assertions below are required.
const directory = mkdtempSync(join(tmpdir(), 'studio-web-browser-'));
const file = join(directory, 'scenario.js');
writeFileSync(file, source);
function cli(args) {
  const result = spawnSync('playwright-cli', [`-s=${session}`, ...args], {
    encoding: 'utf8',
    timeout: 180000,
    maxBuffer: 4 * 1024 * 1024,
  });
  if (
    result.error ||
    result.status !== 0 ||
    result.stdout?.includes('### Error')
  )
    throw new Error(
      'Browser command failed; inspect the local CLI report without publishing authentication artifacts',
    );
  return result.stdout;
}
try {
  cli(['open', 'http://127.0.0.1:3000', '--browser=chrome']);
  const output = cli(['run-code', `--filename=${file}`]);
  const result = output.split('### Result\n')[1]?.split('\n###')[0];
  if (!result?.includes('phase4-web-fixture-pass'))
    throw new Error('Browser scenario did not report verified completion');
  console.log(
    'PostgreSQL/MySQL browser productivity/editing fixture scenario passed. Real OAuth is a separate gate.',
  );
} finally {
  try {
    cli(['close']);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}
