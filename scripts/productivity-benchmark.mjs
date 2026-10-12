import { performance } from 'node:perf_hooks';
import { completions } from '../packages/ui/src/completion.ts';
const names = Array.from({ length: 20000 }, (_, i) => `orders.column_${i}`);
const sql = 'SELECT * FROM orders o WHERE o.';
const samples = [];
for (let i = 0; i < 100; i++) {
  const start = performance.now();
  const result = completions(sql, sql.length, 'Postgres', names);
  if (result.length !== 200)
    throw new Error('Completion count exceeded its contract');
  samples.push(performance.now() - start);
}
samples.sort((a, b) => a - b);
console.log(
  JSON.stringify({
    measured_at: new Date().toISOString(),
    node: process.version,
    metadata_names: names.length,
    samples: samples.length,
    p95_ms: samples[94],
    max_ms: samples[99],
    target_ms: 200,
    scope:
      'pure warm completion; excludes Monaco render, network and cold metadata',
  }),
);
