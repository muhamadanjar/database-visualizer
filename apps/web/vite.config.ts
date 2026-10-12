import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 3000,
    strictPort: true,
    fs: { allow: ['../..'] },
    proxy: {
      '/api': {
        target: process.env.STUDIO_API_PROXY ?? 'http://127.0.0.1:8080',
        ws: true,
      },
      '/health': {
        target: process.env.STUDIO_API_PROXY ?? 'http://127.0.0.1:8080',
      },
    },
  },
  build: { chunkSizeWarningLimit: 1500 },
});
