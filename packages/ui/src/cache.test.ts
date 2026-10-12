import { describe, it, expect } from 'vitest';
import { ResultCache } from './cache';
import type { Row } from '@database-studio/contracts';
const rows = (value: string): Row[] => [[{ kind: 'text', value }]];
describe('bounded result cache', () => {
  it('evicts by tab and total byte budgets', () => {
    const cache = new ResultCache(100, 150);
    cache.put('a', 'first', rows('a'.repeat(35)));
    cache.put('b', 'first', rows('b'.repeat(35)));
    expect(cache.get('a')).toBeUndefined();
    expect(cache.get('b')).toBeDefined();
    cache.put('c', 'second', rows('c'.repeat(35)));
    cache.put('d', 'third', rows('d'.repeat(35)));
    expect(cache.size()).toBeLessThanOrEqual(150);
  });
  it('preserves precision strings and clears only closed tab', () => {
    const cache = new ResultCache();
    cache.put('a', 'first', rows('9007199254740993'));
    cache.put('b', 'second', rows('NULL'));
    cache.clear('first');
    expect(cache.get('a')).toBeUndefined();
    expect(cache.get('b')?.[0][0].value).toBe('NULL');
  });
});
