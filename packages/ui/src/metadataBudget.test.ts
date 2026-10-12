import { expect, it } from 'vitest';
import { QueryClient } from '@tanstack/react-query';
import { enforceMetadataBudget } from './metadataBudget';
it('rejects oversized metadata and leaves another connection usable', () => {
  const client = new QueryClient();
  const unsubscribe = enforceMetadataBudget(client);
  client.setQueryData(['inspect', 'other'], { columns: ['safe'] });
  client.setQueryData(['inspect', 'oversized'], 'x'.repeat(32 * 1024 * 1024));
  expect(client.getQueryData(['inspect', 'oversized'])).toBeUndefined();
  expect(client.getQueryState(['inspect', 'oversized'])?.status).toBe('error');
  expect(client.getQueryData(['inspect', 'other'])).toEqual({
    columns: ['safe'],
  });
  unsubscribe();
  client.clear();
});
