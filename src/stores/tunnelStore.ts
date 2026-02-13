import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import type { TunnelConfig, TunnelConfigInput, TunnelInfo } from '../types/tunnel';

interface TunnelStore {
  // Persistent configs
  configs: TunnelConfig[];
  // Active runtime tunnels
  activeTunnels: TunnelInfo[];
  loading: boolean;
  error: string | null;

  // Config CRUD
  fetchConfigs: (serverId: string) => Promise<void>;
  addConfig: (input: TunnelConfigInput) => Promise<TunnelConfig>;
  updateConfig: (id: string, input: TunnelConfigInput) => Promise<void>;
  deleteConfig: (id: string) => Promise<void>;

  // Runtime tunnel operations
  startTunnel: (sessionId: string, config: TunnelConfigInput) => Promise<TunnelInfo>;
  stopTunnel: (tunnelId: string) => Promise<void>;
  fetchActiveTunnels: (sessionId?: string) => Promise<void>;
  stopAllForSession: (sessionId: string) => Promise<void>;

  clearError: () => void;
}

export const useTunnelStore = create<TunnelStore>((set) => ({
  configs: [],
  activeTunnels: [],
  loading: false,
  error: null,

  fetchConfigs: async (serverId: string) => {
    const result = await safeInvoke<TunnelConfig[]>('tunnel_config_list', { serverId });
    if (result.success) {
      set({ configs: result.data });
    }
  },

  addConfig: async (input: TunnelConfigInput) => {
    set({ loading: true });
    const result = await safeInvoke<TunnelConfig>('tunnel_config_add', { input });
    if (result.success) {
      set((state) => ({
        configs: [...state.configs, result.data],
        loading: false,
      }));
      return result.data;
    } else {
      set({ error: result.error.message, loading: false });
      throw result.error;
    }
  },

  updateConfig: async (id: string, input: TunnelConfigInput) => {
    const result = await safeInvoke('tunnel_config_update', { id, input });
    if (result.success) {
      set((state) => ({
        configs: state.configs.map((c) => (c.id === id ? { ...c, ...input } : c)),
      }));
    }
  },

  deleteConfig: async (id: string) => {
    const result = await safeInvoke('tunnel_config_delete', { id });
    if (result.success) {
      set((state) => ({
        configs: state.configs.filter((c) => c.id !== id),
      }));
    }
  },

  startTunnel: async (sessionId: string, config: TunnelConfigInput) => {
    set({ loading: true });
    const result = await safeInvoke<TunnelInfo>('tunnel_start', { sessionId, config });
    if (result.success) {
      set((state) => ({
        activeTunnels: [...state.activeTunnels, result.data],
        loading: false,
      }));
      return result.data;
    } else {
      set({ error: result.error.message, loading: false });
      throw result.error;
    }
  },

  stopTunnel: async (tunnelId: string) => {
    const result = await safeInvoke('tunnel_stop', { tunnelId });
    if (result.success) {
      set((state) => ({
        activeTunnels: state.activeTunnels.filter((t) => t.id !== tunnelId),
      }));
    }
  },

  fetchActiveTunnels: async (sessionId?: string) => {
    const result = await safeInvoke<TunnelInfo[]>('tunnel_list_active', { sessionId: sessionId || null });
    if (result.success) {
      set({ activeTunnels: result.data });
    }
  },

  stopAllForSession: async (sessionId: string) => {
    await safeInvoke('tunnel_stop_all_for_session', { sessionId });
    set((state) => ({
      activeTunnels: state.activeTunnels.filter((t) => t.sessionId !== sessionId),
    }));
  },

  clearError: () => set({ error: null }),
}));
