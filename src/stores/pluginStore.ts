import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import type {
  PluginExecutionResult,
  PluginInputValues,
  PluginRecord,
} from '../plugins/types';

interface PluginStore {
  plugins: PluginRecord[];
  loading: boolean;
  initialized: boolean;
  operationId: string | null;
  error: string | null;
  fetchPlugins: () => Promise<void>;
  installPlugin: (pluginId: string) => Promise<boolean>;
  importPlugin: () => Promise<PluginRecord | null>;
  uninstallPlugin: (pluginId: string) => Promise<boolean>;
  setPluginEnabled: (pluginId: string, enabled: boolean) => Promise<boolean>;
  updatePluginSettings: (
    pluginId: string,
    settings: Record<string, unknown>
  ) => Promise<boolean>;
  executePluginAction: (
    pluginId: string,
    actionId: string,
    sessionId: string,
    inputs?: PluginInputValues
  ) => Promise<PluginExecutionResult | null>;
  clearError: () => void;
}

let executionRequestSequence = 0;

function replacePlugin(plugins: PluginRecord[], next: PluginRecord): PluginRecord[] {
  const existingIndex = plugins.findIndex(
    (plugin) => plugin.manifest.id === next.manifest.id
  );
  if (existingIndex === -1) return [...plugins, next];
  return plugins.map((plugin, index) => index === existingIndex ? next : plugin);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export const usePluginStore = create<PluginStore>((set, get) => ({
  plugins: [],
  loading: false,
  initialized: false,
  operationId: null,
  error: null,

  fetchPlugins: async () => {
    set({ loading: true, error: null });
    const result = await safeInvoke<PluginRecord[]>('plugin_list');
    if (result.success) {
      set({ plugins: result.data, loading: false, initialized: true });
      return;
    }
    set({
      loading: false,
      initialized: true,
      error: result.error.message,
    });
  },

  installPlugin: async (pluginId) => {
    set({ operationId: pluginId, error: null });
    const result = await safeInvoke<PluginRecord>('plugin_install', {
      request: { pluginId },
    });
    if (result.success) {
      set((state) => ({
        plugins: replacePlugin(state.plugins, result.data),
        operationId: null,
      }));
      return true;
    }
    set({ operationId: null, error: result.error.message });
    return false;
  },

  importPlugin: async () => {
    set({ operationId: 'import', error: null });
    const result = await safeInvoke<PluginRecord | null>('plugin_import');
    if (result.success) {
      if (result.data) {
        set((state) => ({
          plugins: replacePlugin(state.plugins, result.data as PluginRecord),
          operationId: null,
        }));
      } else {
        set({ operationId: null });
      }
      return result.data;
    }
    set({ operationId: null, error: result.error.message });
    return null;
  },

  uninstallPlugin: async (pluginId) => {
    set({ operationId: pluginId, error: null });
    const result = await safeInvoke<void>('plugin_uninstall', {
      request: { pluginId },
    });
    if (!result.success) {
      set({ operationId: null, error: result.error.message });
      return false;
    }

    await get().fetchPlugins();
    set({ operationId: null });
    return true;
  },

  setPluginEnabled: async (pluginId, enabled) => {
    set({ operationId: pluginId, error: null });
    const result = await safeInvoke<PluginRecord>('plugin_set_enabled', {
      request: { pluginId, enabled },
    });
    if (result.success) {
      set((state) => ({
        plugins: replacePlugin(state.plugins, result.data),
        operationId: null,
      }));
      return true;
    }
    set({ operationId: null, error: result.error.message });
    return false;
  },

  updatePluginSettings: async (pluginId, settings) => {
    const result = await safeInvoke<PluginRecord>('plugin_update_settings', {
      request: { pluginId, settings },
    });
    if (result.success) {
      set((state) => ({
        plugins: replacePlugin(state.plugins, result.data),
      }));
      return true;
    }
    set({ error: result.error.message });
    return false;
  },

  executePluginAction: async (pluginId, actionId, sessionId, inputs = {}) => {
    const requestSequence = ++executionRequestSequence;
    set({ operationId: `${pluginId}:${actionId}`, error: null });
    try {
      const result = await safeInvoke<PluginExecutionResult>('plugin_execute', {
        request: { pluginId, actionId, sessionId, inputs },
      });
      if (result.success) {
        if (requestSequence === executionRequestSequence) {
          set({ operationId: null });
        }
        return result.data;
      }
      if (requestSequence === executionRequestSequence) {
        set({ operationId: null, error: result.error.message });
      }
      return null;
    } catch (error) {
      if (requestSequence === executionRequestSequence) {
        set({ operationId: null, error: errorMessage(error) });
      }
      return null;
    }
  },

  clearError: () => {
    executionRequestSequence += 1;
    set({ error: null, operationId: null });
  },
}));
