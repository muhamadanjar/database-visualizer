import type { Engine } from '@database-studio/contracts';
const common = [
  'SELECT',
  'FROM',
  'WHERE',
  'JOIN',
  'LEFT JOIN',
  'GROUP BY',
  'ORDER BY',
  'LIMIT',
  'INSERT INTO',
  'UPDATE',
  'DELETE FROM',
  'CREATE TABLE',
  'ALTER TABLE',
  'BEGIN',
  'COMMIT',
  'ROLLBACK',
  'EXPLAIN',
  'WITH',
  'HAVING',
  'DISTINCT',
  'AS',
  'IS NULL',
];
export function completions(
  sql: string,
  cursor: number,
  engine: Engine,
  names: readonly string[],
): string[] {
  const source = sql.slice(Math.max(0, cursor - 65536), cursor);
  let literal: 'single' | 'line' | 'block' | null = null;
  let identifier: '"' | '`' | null = null;
  let dollar: string | null = null;
  for (let i = 0; i < source.length; i++) {
    const c = source[i],
      next = source[i + 1];
    if (dollar) {
      if (source.startsWith(dollar, i)) {
        i += dollar.length - 1;
        dollar = null;
      }
      continue;
    }
    if (literal === 'line') {
      if (c === '\n') literal = null;
      continue;
    }
    if (literal === 'block') {
      if (c === '*' && next === '/') {
        literal = null;
        i++;
      }
      continue;
    }
    if (literal === 'single') {
      if (c === "'") {
        if (next === "'") i++;
        else literal = null;
      } else if (c === '\\' && engine === 'MySql') i++;
      continue;
    }
    if (identifier) {
      if (c === identifier) {
        if (next === identifier) i++;
        else identifier = null;
      }
      continue;
    }
    if (c === "'") literal = 'single';
    else if (c === '"' || c === '`') identifier = c;
    else if (c === '-' && next === '-') {
      literal = 'line';
      i++;
    } else if (c === '/' && next === '*') {
      literal = 'block';
      i++;
    } else if (c === '#' && engine === 'MySql') literal = 'line';
    else if (c === '$' && engine === 'Postgres') {
      const tag = source.slice(i).match(/^\$(?:[A-Za-z_][\w]*)?\$/)?.[0];
      if (tag) {
        dollar = tag;
        i += tag.length - 1;
      }
    }
  }
  if (literal || dollar) return [];
  const tokens =
    source.match(
      /--[^\n]*|\/\*[\s\S]*?\*\/|'(?:''|[^'])*'|"(?:""|[^"])*"|`(?:``|[^`])*`|[A-Za-z_][\w$]*|[.;,()]/g,
    ) ?? [];
  // Strings/comments are never completion contexts; SQL text is never rewritten.
  const clean = tokens.filter(
    (t) => !t.startsWith('--') && !t.startsWith('/*') && !t.startsWith("'"),
  );
  const start = clean.lastIndexOf(';');
  const statement = clean.slice(start + 1);
  const aliases = new Map<string, string>();
  const reserved = new Set([
    'WHERE',
    'ON',
    'JOIN',
    'LEFT',
    'RIGHT',
    'INNER',
    'OUTER',
    'GROUP',
    'ORDER',
    'LIMIT',
    'HAVING',
    'SET',
    'VALUES',
  ]);
  for (let i = 0; i < statement.length; i++)
    if (
      ['FROM', 'JOIN', 'UPDATE', 'INTO'].includes(statement[i].toUpperCase())
    ) {
      let table = statement[++i]?.replace(/^["`]|["`]$/g, '');
      if (!table || table === '(') continue;
      if (statement[i + 1] === '.') {
        table += '.' + (statement[i + 2] ?? '').replace(/^["`]|["`]$/g, '');
        i += 2;
      }
      aliases.set(table.toLowerCase(), table);
      aliases.set(table.split('.').at(-1)!.toLowerCase(), table);
      if (statement[i + 1]?.toUpperCase() === 'AS') i++;
      const alias = statement[i + 1]?.replace(/^["`]|["`]$/g, '');
      if (
        alias &&
        !reserved.has(alias.toUpperCase()) &&
        !['.', ',', ')', ';'].includes(alias)
      )
        aliases.set(alias.toLowerCase(), table);
    }
  const suffix = source.match(/([\w$]+)\.([\w$]*)$/);
  let values: string[];
  if (suffix) {
    const target = aliases.get(suffix[1].toLowerCase()) ?? suffix[1];
    values = names
      .filter(
        (n) =>
          n.toLowerCase().startsWith(target.toLowerCase() + '.') ||
          (!target.includes('.') &&
            n
              .toLowerCase()
              .startsWith(target.split('.').at(-1)!.toLowerCase() + '.')),
      )
      .map((n) => n.slice(n.lastIndexOf('.') + 1));
  } else
    values = [
      ...common,
      ...(engine === 'Postgres'
        ? ['RETURNING', 'ILIKE', 'ON CONFLICT']
        : ['SHOW', 'DESCRIBE', 'ON DUPLICATE KEY UPDATE']),
      ...names,
      ...aliases.keys(),
    ];
  const prefix = suffix?.[2] ?? source.match(/[\w$]*$/)?.[0] ?? '';
  return [...new Set(values)]
    .filter((n) => n.toLowerCase().startsWith(prefix.toLowerCase()))
    .slice(0, 200);
}
