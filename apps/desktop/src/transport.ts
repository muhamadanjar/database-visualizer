import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  DatabaseTransport,
  Notification,
} from '@database-studio/contracts';
export class TauriDatabaseTransport implements DatabaseTransport {
  private metadataTail = new Map<string, Promise<unknown>>();
  private metadataPending = new Map<string, number>();
  async onCloseRequested(handler: () => Promise<boolean>): Promise<() => void> {
    if (!isTauri()) return () => {};
    let closing = false;
    let pending = false;
    return getCurrentWindow().onCloseRequested(async (event) => {
      if (closing) return;
      event.preventDefault();
      if (pending) return;
      pending = true;
      try {
        if (await handler()) {
          await this.request('shutdown');
          closing = true;
          await getCurrentWindow().close();
        }
      } finally {
        pending = false;
      }
    });
  }
  async request<T>(
    operation: string,
    args: Record<string, unknown> = {},
  ): Promise<T> {
    if (!isTauri())
      throw new Error(
        'Jalankan aplikasi desktop untuk mengakses database. Preview browser tidak memiliki koneksi database.',
      );
    if (operation === 'metadata') {
      const id = String(args.connection_id);
      const pending = this.metadataPending.get(id) ?? 0;
      if (pending >= 16)
        throw new Error('Tunggu pemuatan metadata saat ini selesai.');
      this.metadataPending.set(id, pending + 1);
      const job = (this.metadataTail.get(id) ?? Promise.resolve())
        .catch(() => {})
        .then(() =>
          invoke<T>('desktop_request', { request: { operation, args } }),
        );
      this.metadataTail.set(id, job);
      try {
        return await job;
      } finally {
        this.metadataPending.set(id, (this.metadataPending.get(id) ?? 1) - 1);
        if (this.metadataTail.get(id) === job) {
          this.metadataTail.delete(id);
          this.metadataPending.delete(id);
        }
      }
    }
    return invoke<T>('desktop_request', { request: { operation, args } });
  }
  async watch(
    executionId: string,
    onEvent: (event: Notification) => void,
  ): Promise<() => void> {
    let active = true;
    const channel = new Channel<Notification>();
    channel.onmessage = (event) => {
      if (active) onEvent(event);
      void this.request('watch_ack', {
        execution_id: executionId,
        sequence: event.sequence,
      }).catch(() => {});
    };
    await invoke('watch_execution', {
      executionId,
      version: 1,
      onEvent: channel,
    });
    return () => {
      active = false;
    };
  }
}
