import type { QueryClient } from '@tanstack/react-query';
const metadata = (key: readonly unknown[]) =>
  ['schemas', 'objects', 'inspect'].includes(String(key[0]));
export function enforceMetadataBudget(client: QueryClient) {
  return client.getQueryCache().subscribe((event) => {
    if (
      event.type !== 'updated' ||
      event.action.type !== 'success' ||
      !metadata(event.query.queryKey)
    )
      return;
    const sizes = () =>
      client
        .getQueryCache()
        .getAll()
        .filter((q) => metadata(q.queryKey) && q.state.data !== undefined)
        .map((q) => ({
          q,
          bytes: new TextEncoder().encode(JSON.stringify(q.state.data)).length,
        }));
    const reject = () =>
      event.query.setState({
        data: undefined,
        status: 'error',
        error: new Error(
          'Cache metadata penuh. Tutup inspector/cabang explorer atau refresh koneksi sebelum memuat lagi.',
        ),
      });
    let records = sizes();
    if (
      (records.find((r) => r.q === event.query)?.bytes ?? 0) >
      32 * 1024 * 1024
    ) {
      reject();
      return;
    }
    const connection = String(event.query.queryKey[1]);
    const over = () =>
      records.reduce((n, r) => n + r.bytes, 0) > 128 * 1024 * 1024 ||
      records
        .filter((r) => String(r.q.queryKey[1]) === connection)
        .reduce((n, r) => n + r.bytes, 0) >
        32 * 1024 * 1024;
    for (const candidate of [...records]
      .filter((r) => r.q !== event.query && !r.q.getObserversCount())
      .sort((a, b) => a.q.state.dataUpdatedAt - b.q.state.dataUpdatedAt)) {
      if (!over()) break;
      client.removeQueries({ queryKey: candidate.q.queryKey, exact: true });
      records = sizes();
    }
    if (over()) reject();
  });
}
