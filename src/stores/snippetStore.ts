import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import type { CommandSnippet, CreateSnippetInput } from '../types/tunnel';

interface SnippetStore {
  snippets: CommandSnippet[];
  loading: boolean;
  error: string | null;

  fetchSnippets: (category?: string) => Promise<void>;
  addSnippet: (input: CreateSnippetInput) => Promise<CommandSnippet>;
  updateSnippet: (id: string, updates: Partial<CreateSnippetInput>) => Promise<void>;
  deleteSnippet: (id: string) => Promise<void>;
  searchSnippets: (query: string) => Promise<void>;
  clearError: () => void;
}

export const useSnippetStore = create<SnippetStore>((set) => ({
  snippets: [],
  loading: false,
  error: null,

  fetchSnippets: async (category?: string) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<CommandSnippet[]>('snippet_list', { category: category || null });
    if (result.success) {
      set({ snippets: result.data, loading: false });
    } else {
      set({ snippets: [], loading: false, error: result.error.message });
    }
  },

  addSnippet: async (input: CreateSnippetInput) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<CommandSnippet>('snippet_add', { input });
    if (result.success) {
      set((state) => ({
        snippets: [result.data, ...state.snippets],
        loading: false,
      }));
      return result.data;
    } else {
      set({ error: result.error.message, loading: false });
      throw result.error;
    }
  },

  updateSnippet: async (id: string, updates: Partial<CreateSnippetInput>) => {
    set({ loading: true, error: null });
    const result = await safeInvoke('snippet_update', { input: { id, ...updates } });
    if (result.success) {
      set((state) => ({
        snippets: state.snippets.map((s) =>
          s.id === id ? { ...s, ...updates, updated_at: Date.now() } : s
        ),
        loading: false,
      }));
    } else {
      set({ error: result.error.message, loading: false });
    }
  },

  deleteSnippet: async (id: string) => {
    set({ loading: true, error: null });
    const result = await safeInvoke('snippet_delete', { id });
    if (result.success) {
      set((state) => ({
        snippets: state.snippets.filter((s) => s.id !== id),
        loading: false,
      }));
    } else {
      set({ error: result.error.message, loading: false });
    }
  },

  searchSnippets: async (query: string) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<CommandSnippet[]>('snippet_search', { query });
    if (result.success) {
      set({ snippets: result.data, loading: false });
    } else {
      set({ error: result.error.message, loading: false });
    }
  },

  clearError: () => set({ error: null }),
}));
