import { expect, it } from 'vitest';
import { completions } from './completion';
it('resolves qualified aliases without mixing columns from other tables', () => {
  const sql = 'SELECT * FROM public.users AS u JOIN orders o ON u.';
  expect(
    completions(sql, sql.length, 'Postgres', [
      'public.users.id',
      'public.users.name',
      'orders.total',
    ]),
  ).toEqual(['id', 'name']);
});
it('bounds suggestions and preserves dialect keywords', () => {
  expect(
    completions(
      '',
      0,
      'Postgres',
      Array.from({ length: 1000 }, (_, i) => 't' + i),
    ),
  ).toHaveLength(200);
  expect(completions('ILI', 3, 'Postgres', [])).toContain('ILIKE');
  expect(completions('ILI', 3, 'MySql', [])).not.toContain('ILIKE');
});
it('limits alias resolution to the active statement', () => {
  const sql = 'SELECT * FROM users u; SELECT u.';
  expect(completions(sql, sql.length, 'Postgres', ['users.secret'])).toEqual(
    [],
  );
});

it('does not complete SQL strings or unfinished comments', () => {
  for (const sql of [
    "SELECT 'FROM users u u.",
    'SELECT /* users.',
    'SELECT -- users.',
    'SELECT $body$users.',
  ])
    expect(completions(sql, sql.length, 'Postgres', ['users.id'])).toEqual([]);
});

it('measures bounded warm completion with a large metadata catalog', () => {
  const names = Array.from({ length: 20000 }, (_, i) => `orders.column_${i}`);
  const sql = 'SELECT * FROM orders o WHERE o.';
  const samples: number[] = [];
  for (let i = 0; i < 50; i++) {
    const start = performance.now();
    const result = completions(sql, sql.length, 'Postgres', names);
    samples.push(performance.now() - start);
    expect(result.length).toBe(200);
  }
  samples.sort((a, b) => a - b);
  console.info(
    `Warm completion local fixture: 20000 names, 50 samples, p95=${samples[47].toFixed(2)}ms, max=${samples[49].toFixed(2)}ms`,
  );
});
