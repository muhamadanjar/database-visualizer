import { defineConfig } from 'vitest/config';
export default defineConfig({
  test: {
    environment: 'jsdom',
    include: ['packages/**/*.test.{ts,tsx}', 'apps/web/src/**/*.test.{ts,tsx}'],
    setupFiles: ['./tests/frontend/setup.ts'],
  },
});
