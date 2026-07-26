import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';
import { refreshCloudSyncEntities } from './cloudSyncCoordinator';

export type CloudSyncProvider = 'github_gist' | 'webdav';

export interface CreateCloudSyncVaultInput {
  provider: CloudSyncProvider;
  endpoint?: string;
  gistId?: string;
  token?: string;
  username?: string;
  password?: string;
}

export interface CloudSyncPairingInfo {
  provider: CloudSyncProvider;
  endpoint: string;
  vaultId: string;
  pairingCode: string;
}

export interface CloudSyncStatus {
  unlocked: boolean;
  syncing: boolean;
  provider: CloudSyncProvider | null;
  endpoint: string | null;
  vaultId: string | null;
  pendingChanges: number;
  conflicts: number;
  lastSuccessAt: number | null;
  lastError: string | null;
}

export interface CloudSyncReport {
  uploaded: number;
  downloaded: number;
  applied: number;
  ignored: number;
  conflicts: number;
  pendingChanges: number;
  cursor: string | null;
}

export interface CloudSyncFileReport {
  operation: 'export' | 'import';
  path: string;
  exported: number;
  imported: number;
  applied: number;
  ignored: number;
  conflicts: number;
}

const LOCKED_STATUS: CloudSyncStatus = {
  unlocked: false,
  syncing: false,
  provider: null,
  endpoint: null,
  vaultId: null,
  pendingChanges: 0,
  conflicts: 0,
  lastSuccessAt: null,
  lastError: null,
};

let scheduledSync: ReturnType<typeof setTimeout> | null = null;

export function scheduleCloudSync(delayMs = 750): void {
  if (!useCloudSyncStore.getState().status.unlocked) return;
  if (scheduledSync !== null) clearTimeout(scheduledSync);

  const run = () => {
    const state = useCloudSyncStore.getState();
    if (!state.status.unlocked) {
      scheduledSync = null;
      return;
    }
    if (state.loading || state.status.syncing) {
      scheduledSync = setTimeout(run, delayMs);
      return;
    }
    scheduledSync = null;
    void state.syncNow();
  };

  scheduledSync = setTimeout(run, delayMs);
}

interface CloudSyncStore {
  status: CloudSyncStatus;
  pairingInfo: CloudSyncPairingInfo | null;
  report: CloudSyncReport | null;
  fileReport: CloudSyncFileReport | null;
  loading: boolean;
  error: string | null;
  refreshStatus: () => Promise<void>;
  createVault: (input: CreateCloudSyncVaultInput) => Promise<CloudSyncPairingInfo | null>;
  joinVault: (pairingCode: string) => Promise<CloudSyncPairingInfo | null>;
  syncNow: () => Promise<CloudSyncReport | null>;
  lock: () => Promise<void>;
  exportFile: () => Promise<CloudSyncFileReport | null>;
  importFile: () => Promise<CloudSyncFileReport | null>;
  clearPairingInfo: () => void;
}

export const useCloudSyncStore = create<CloudSyncStore>((set, get) => ({
  status: LOCKED_STATUS,
  pairingInfo: null,
  report: null,
  fileReport: null,
  loading: false,
  error: null,

  refreshStatus: async () => {
    const result = await safeInvoke<CloudSyncStatus>('cloud_sync_status');
    if (result.success) {
      set({ status: result.data, error: result.data.lastError });
    } else if (!result.error.isTauriUnavailable) {
      set({ error: result.error.message });
    }
  },

  createVault: async (input) => {
    set({ loading: true, error: null, pairingInfo: null, report: null, fileReport: null });
    const result = await safeInvoke<CloudSyncPairingInfo>('cloud_sync_create_vault', {
      request: input,
    });
    if (!result.success) {
      set({ loading: false, error: result.error.message });
      return null;
    }
    set({ loading: false, pairingInfo: result.data });
    await get().refreshStatus();
    await get().syncNow();
    return result.data;
  },

  joinVault: async (pairingCode) => {
    set({ loading: true, error: null, pairingInfo: null, report: null });
    const result = await safeInvoke<CloudSyncPairingInfo>('cloud_sync_join_vault', {
      request: { pairingCode },
    });
    if (!result.success) {
      set({ loading: false, error: result.error.message });
      return null;
    }
    set({ loading: false });
    await get().refreshStatus();
    await get().syncNow();
    return result.data;
  },

  syncNow: async () => {
    const current = get();
    if (current.loading || current.status.syncing) return null;
    set((state) => ({
      loading: true,
      error: null,
      status: { ...state.status, syncing: true },
    }));
    const result = await safeInvoke<CloudSyncReport>('cloud_sync_now');
    if (!result.success) {
      set({ loading: false, error: result.error.message });
      await get().refreshStatus();
      return null;
    }
    set({ loading: false, report: result.data });
    if (result.data.downloaded > 0) {
      await refreshCloudSyncEntities();
    }
    await get().refreshStatus();
    return result.data;
  },

  lock: async () => {
    const result = await safeInvoke<void>('cloud_sync_lock');
    if (!result.success) {
      set({ error: result.error.message });
      return;
    }
    if (scheduledSync !== null) {
      clearTimeout(scheduledSync);
      scheduledSync = null;
    }
    set({
      status: LOCKED_STATUS,
      pairingInfo: null,
      report: null,
      fileReport: null,
      loading: false,
      error: null,
    });
  },

  exportFile: async () => {
    set({ loading: true, error: null, fileReport: null });
    const result = await safeInvoke<CloudSyncFileReport | null>('cloud_sync_export_file');
    if (!result.success) {
      set({ loading: false, error: result.error.message });
      return null;
    }
    set({ loading: false, fileReport: result.data });
    return result.data;
  },

  importFile: async () => {
    set({ loading: true, error: null, fileReport: null });
    const result = await safeInvoke<CloudSyncFileReport | null>('cloud_sync_import_file');
    if (!result.success) {
      set({ loading: false, error: result.error.message });
      return null;
    }
    set({ loading: false, fileReport: result.data });
    if (result.data && result.data.applied > 0) {
      await refreshCloudSyncEntities();
    }
    return result.data;
  },

  clearPairingInfo: () => set({ pairingInfo: null }),
}));
