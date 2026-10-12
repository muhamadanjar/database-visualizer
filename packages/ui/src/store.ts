import { create } from 'zustand';
import type { ObjectId, Session, Execution } from '@database-studio/contracts';
export interface Tab {
  id: string;
  connectionId: string;
  title: string;
  sql: string;
  savedId?: string;
  session?: Session;
  execution?: Execution;
  object?: ObjectId;
  inspector?: boolean;
}
interface State {
  tabs: Tab[];
  activeId: string | null;
  sidebarWidth: number;
  editorHeight: number;
  add: (tab: Tab) => void;
  patch: (id: string, patch: Partial<Tab>) => void;
  remove: (id: string) => void;
  activate: (id: string) => void;
  layout: (
    values: Partial<Pick<State, 'sidebarWidth' | 'editorHeight'>>,
  ) => void;
}
export const useWorkspace = create<State>((set) => ({
  tabs: [],
  activeId: null,
  sidebarWidth: 248,
  editorHeight: 330,
  add: (tab) => set((s) => ({ tabs: [...s.tabs, tab], activeId: tab.id })),
  patch: (id, patch) =>
    set((s) => ({
      tabs: s.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)),
    })),
  remove: (id) =>
    set((s) => {
      const tabs = s.tabs.filter((t) => t.id !== id);
      return {
        tabs,
        activeId: s.activeId === id ? (tabs.at(-1)?.id ?? null) : s.activeId,
      };
    }),
  activate: (activeId) => set({ activeId }),
  layout: (values) => set(values),
}));
