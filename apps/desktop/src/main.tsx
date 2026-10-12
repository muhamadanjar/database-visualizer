import React from 'react';
import { createRoot } from 'react-dom/client';
import { Workspace } from '@database-studio/workspace';
import { TauriDatabaseTransport } from './transport';
import '@database-studio/workspace/styles.css';
async function bootstrap() {
  if (import.meta.env.VITE_DESKTOP_E2E === '1')
    await import('@wdio/tauri-plugin');
  createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Workspace transport={new TauriDatabaseTransport()} />
    </React.StrictMode>,
  );
}
void bootstrap();
