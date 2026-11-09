import React from 'react';
import { createRoot } from 'react-dom/client';
import { Channel, invoke } from '@tauri-apps/api/core';
import { Workspace } from '@database-studio/workspace';
import { TauriDatabaseTransport } from './transport';
import type { Notification } from '@database-studio/contracts';
import '@database-studio/workspace/styles.css';
async function bootstrap() {
  if (import.meta.env.VITE_DESKTOP_E2E === '1') {
    await import('@wdio/tauri-plugin');
    Object.assign(globalThis, {
      __DATABASE_STUDIO_WATCH_NO_ACK__: async (
        executionId: string,
        slot: string,
      ) => {
        const events: Notification[] = [];
        Object.assign(globalThis, { [slot]: events });
        const channel = new Channel<Notification>((event) =>
          events.push(event),
        );
        await invoke('watch_execution', {
          executionId,
          version: 1,
          onEvent: channel,
        });
      },
    });
  }
  createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Workspace transport={new TauriDatabaseTransport()} />
    </React.StrictMode>,
  );
}
void bootstrap();
