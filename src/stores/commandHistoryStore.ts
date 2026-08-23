import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import type { CommandHistoryEntry } from '../types/commandHistory';

interface CommandHistoryStore {
  entries: CommandHistoryEntry[];
  activeServerId: string | null;
  loading: boolean;
  error: string | null;

  fetchHistory: (serverId: string | null, query?: string, favoritesOnly?: boolean) => Promise<void>;
  recordCommand: (serverId: string, command: string) => Promise<CommandHistoryEntry | null>;
  setFavorite: (id: string, isFavorite: boolean) => Promise<boolean>;
  deleteEntry: (id: string) => Promise<boolean>;
  clearHistory: (serverId: string, includeFavorites?: boolean) => Promise<boolean>;
  clearError: () => void;
}

let fetchRequestId = 0;

function sortEntries(entries: CommandHistoryEntry[]): CommandHistoryEntry[] {
  return [...entries].sort((a, b) => {
    if (a.is_favorite !== b.is_favorite) return a.is_favorite ? -1 : 1;
    return b.last_used_at - a.last_used_at;
  });
}

export const useCommandHistoryStore = create<CommandHistoryStore>((set, get) => ({
  entries: [],
  activeServerId: null,
  loading: false,
  error: null,

  fetchHistory: async (serverId, query = '', favoritesOnly = false) => {
    const requestId = ++fetchRequestId;
    if (!serverId) {
      set({ entries: [], activeServerId: null, loading: false, error: null });
      return;
    }

    set({ activeServerId: serverId, loading: true, error: null });
    const result = await safeInvoke<CommandHistoryEntry[]>('history_list', {
      input: {
        serverId,
        query: query.trim() || null,
        favoritesOnly,
        limit: 200,
      },
    });
    if (requestId !== fetchRequestId || get().activeServerId !== serverId) return;

    if (result.success) {
      set({ entries: sortEntries(result.data), loading: false });
    } else {
      set({ entries: [], loading: false, error: result.error.message });
    }
  },

  recordCommand: async (serverId, command) => {
    const normalized = command.trim();
    if (!normalized) return null;

    const result = await safeInvoke<CommandHistoryEntry>('history_record', {
      input: { serverId, command: normalized },
    });
    if (!result.success) {
      set({ error: result.error.message });
      return null;
    }

    if (get().activeServerId === serverId) {
      set((state) => ({
        entries: sortEntries([
          result.data,
          ...state.entries.filter((entry) => entry.id !== result.data.id),
        ]),
      }));
    }
    return result.data;
  },

  setFavorite: async (id, isFavorite) => {
    const result = await safeInvoke('history_set_favorite', {
      input: { id, isFavorite },
    });
    if (!result.success) {
      set({ error: result.error.message });
      return false;
    }

    set((state) => ({
      entries: sortEntries(
        state.entries.map((entry) =>
          entry.id === id ? { ...entry, is_favorite: isFavorite } : entry
        )
      ),
    }));
    return true;
  },

  deleteEntry: async (id) => {
    const result = await safeInvoke('history_delete', { id });
    if (!result.success) {
      set({ error: result.error.message });
      return false;
    }
    set((state) => ({ entries: state.entries.filter((entry) => entry.id !== id) }));
    return true;
  },

  clearHistory: async (serverId, includeFavorites = false) => {
    const result = await safeInvoke('history_clear', {
      input: { serverId, includeFavorites },
    });
    if (!result.success) {
      set({ error: result.error.message });
      return false;
    }
    set((state) => ({
      entries: includeFavorites
        ? []
        : state.entries.filter((entry) => entry.is_favorite),
    }));
    return true;
  },

  clearError: () => set({ error: null }),
}));
