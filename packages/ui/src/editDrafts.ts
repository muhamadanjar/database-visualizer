import { create } from 'zustand';
import type {
  DatabaseTransport,
  ColumnMetadata,
  Row,
} from '@database-studio/contracts';
export interface EditSnapshot {
  token: string;
  profile_revision?: number;
  offset: number;
  columns: ColumnMetadata[];
  rows: Row[];
  primary_key: string[];
  generated: string[];
  warnings: string[];
}
export type Input =
  | { type: 'default' }
  | { type: 'value'; value: string | null };
export type Change =
  | { type: 'insert'; values: Record<string, Input> }
  | { type: 'update'; row: number; values: Record<string, Input> }
  | { type: 'delete'; row: number };
export interface Draft {
  connection: string;
  snapshot: EditSnapshot;
  changes: Change[];
  state: string;
  key?: string;
  mutation?: string;
  readback?: Row[];
  preview?: { statements: string[]; warnings: string[] };
}
export const useEditDrafts = create<{
  drafts: Record<string, Draft>;
  put: (id: string, draft: Draft) => void;
  remove: (id: string) => void;
}>((set) => ({
  drafts: {},
  put: (id, draft) =>
    set((state) => ({ drafts: { ...state.drafts, [id]: draft } })),
  remove: (id) =>
    set((state) => {
      const drafts = { ...state.drafts };
      delete drafts[id];
      return { drafts };
    }),
}));
export const editingDirty = () =>
  Object.values(useEditDrafts.getState().drafts).some(
    (d) =>
      d.changes.length > 0 ||
      ['Applying', 'Refreshing', 'Unknown'].includes(d.state),
  );
export async function discardEditDrafts(
  transport: DatabaseTransport,
  id?: string,
): Promise<boolean> {
  for (const [tab, draft] of Object.entries(useEditDrafts.getState().drafts)) {
    if (id && tab !== id) continue;
    if (['Applying', 'Refreshing'].includes(draft.state)) {
      window.alert('Batalkan atau tunggu Apply selesai sebelum menutup tab.');
      return false;
    }
    if (
      (draft.changes.length || draft.state === 'Unknown') &&
      !window.confirm(
        draft.state === 'Unknown'
          ? 'Commit belum diketahui. Periksa data melalui query sebelum membuat Apply baru. Tutup dan hapus draft?'
          : 'Buang perubahan tabel yang belum diterapkan?',
      )
    )
      return false;
    await transport.request('edit_discard', {
      connection_id: draft.connection,
      tab_id: tab,
    });
    const current = useEditDrafts.getState().drafts[tab];
    if (current !== draft || ['Applying', 'Refreshing'].includes(current.state))
      return false;
    useEditDrafts.getState().remove(tab);
  }
  return true;
}
