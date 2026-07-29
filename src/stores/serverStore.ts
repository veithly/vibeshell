import { create } from 'zustand';
import { useMemo } from 'react';
import { safeInvoke, TauriError } from '../lib/tauri';
import { useNotificationStore } from './notificationStore';
import { useSessionStore } from './sessionStore';
import { scheduleCloudSync } from './cloudSyncStore';

/**
 * Helper to show error notification
 */
function showError(title: string, error: TauriError): void {
  const { error: notifyError } = useNotificationStore.getState();
  // Skip notification for Tauri unavailable errors (handled by banner)
  if (!error.isTauriUnavailable) {
    notifyError(title, error.message);
  }
}

/**
 * Authentication type for SSH connections
 * Matches backend AuthType enum (snake_case).
 *
 * Standalone 'key' auth has been removed: key-based servers always use
 * 'key_with_passphrase', where an empty passphrase means an unencrypted key.
 * The 'key' member is kept only so legacy rows still type-check when read.
 */
export type AuthType = 'password' | 'key' | 'key_with_passphrase';

/**
 * Returns true when the auth type is key-based (either legacy 'key' rows or
 * the unified 'key_with_passphrase' mode).
 */
export function isKeyAuthType(authType: AuthType): boolean {
  return authType === 'key' || authType === 'key_with_passphrase';
}

/**
 * Server configuration for SSH connections
 * Matches backend Server model
 */
export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_type: AuthType;
  credential_id?: string | null;
  group_id?: string | null;
  tags: string[];
  created_at: number;
  updated_at: number;
  /** Jump host server ID for ProxyJump connections */
  jump_host_id?: string | null;
  /** Command to auto-execute after SSH login */
  post_login_command?: string | null;
  /** Whether to enable SSH agent forwarding */
  agent_forwarding?: boolean;
}

/**
 * Server group for organizing servers
 * Matches backend Group model
 */
export interface Group {
  id: string;
  name: string;
  parent_id?: string;
  color: string;
}

/**
 * Input for creating a new server (omits auto-generated fields)
 */
export type CreateServerInput = Omit<Server, 'id' | 'created_at' | 'updated_at'>;

/**
 * Input for creating a new group
 */
export type CreateGroupInput = Omit<Group, 'id'>;

/**
 * Server store state and actions
 */
interface ServerStore {
  // State
  servers: Server[];
  groups: Group[];
  selectedServerId: string | null;
  loading: boolean;
  error: string | null;

  // Server actions
  fetchServers: () => Promise<void>;
  addServer: (server: CreateServerInput) => Promise<Server>;
  updateServer: (id: string, updates: Partial<CreateServerInput>) => Promise<void>;
  deleteServer: (id: string) => Promise<void>;
  selectServer: (id: string | null) => void;

  // Group actions
  fetchGroups: () => Promise<void>;
  addGroup: (group: CreateGroupInput) => Promise<Group>;
  deleteGroup: (id: string) => Promise<void>;

  // Utility actions
  clearError: () => void;
  getServersByGroup: (groupId: string | null) => Server[];
}

/**
 * Zustand store for managing server and group state
 */
export const useServerStore = create<ServerStore>((set, get) => ({
  // Initial state
  servers: [],
  groups: [],
  selectedServerId: null,
  loading: false,
  error: null,

  // Server actions
  fetchServers: async () => {
    set({ loading: true, error: null });
    const result = await safeInvoke<Server[]>('get_servers');
    if (result.success) {
      set({ servers: result.data, loading: false });
    } else {
      // Set empty state but track the error
      const errorMsg = result.error.isTauriUnavailable
        ? 'Running in browser mode'
        : result.error.message;
      set({ servers: [], loading: false, error: errorMsg });
      // Don't show toast for initial load failures when Tauri unavailable
      if (!result.error.isTauriUnavailable) {
        showError('Failed to Load Servers', result.error);
      }
    }
  },

  addServer: async (serverInput: CreateServerInput) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<Server>('add_server', { server: serverInput });
    if (result.success) {
      set((state) => ({
        servers: [...state.servers, result.data],
        loading: false,
      }));
      scheduleCloudSync();
      return result.data;
    } else {
      set({ error: result.error.message, loading: false });
      showError('Failed to Add Server', result.error);
      throw result.error;
    }
  },

  updateServer: async (id: string, updates: Partial<CreateServerInput>) => {
    set({ loading: true, error: null });
    const result = await safeInvoke('update_server', { id, updates });
    if (result.success) {
      // Optimistically merge so the UI reflects the change immediately.
      set((state) => ({
        servers: state.servers.map((s) =>
          s.id === id ? { ...s, ...updates, updated_at: Math.floor(Date.now() / 1000) } : s
        ),
        loading: false,
      }));
      // Re-fetch from the backend so the local cache mirrors the database
      // exactly (server-side timestamps, credential migrations, etc.).
      await get().fetchServers();
      scheduleCloudSync();
    } else {
      set({ error: result.error.message, loading: false });
      showError('Failed to Update Server', result.error);
    }
  },

  deleteServer: async (id: string) => {
    set({ loading: true, error: null });
    const result = await safeInvoke('delete_server', { id });
    if (result.success) {
      set((state) => ({
        servers: state.servers.filter((s) => s.id !== id),
        selectedServerId: state.selectedServerId === id ? null : state.selectedServerId,
        loading: false,
      }));
      // The backend killed every session attached to this server; refresh the
      // session store so closed tabs disappear immediately instead of waiting
      // for the next poll.
      void useSessionStore.getState().syncRemoteSessions();
      scheduleCloudSync();
    } else {
      set({ error: result.error.message, loading: false });
      showError('Failed to Delete Server', result.error);
    }
  },

  selectServer: (id: string | null) => {
    set({ selectedServerId: id });
  },

  // Group actions
  fetchGroups: async () => {
    set({ loading: true, error: null });
    const result = await safeInvoke<Group[]>('get_groups');
    if (result.success) {
      set({ groups: result.data, loading: false });
    } else {
      // Set empty state but track the error
      const errorMsg = result.error.isTauriUnavailable
        ? 'Running in browser mode'
        : result.error.message;
      set({ groups: [], loading: false, error: errorMsg });
      // Don't show toast for initial load failures when Tauri unavailable
      if (!result.error.isTauriUnavailable) {
        showError('Failed to Load Groups', result.error);
      }
    }
  },

  addGroup: async (groupInput: CreateGroupInput) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<Group>('add_group', { group: groupInput });
    if (result.success) {
      set((state) => ({
        groups: [...state.groups, result.data],
        loading: false,
      }));
      scheduleCloudSync();
      return result.data;
    } else {
      set({ error: result.error.message, loading: false });
      showError('Failed to Add Group', result.error);
      throw result.error;
    }
  },

  deleteGroup: async (id: string) => {
    set({ loading: true, error: null });
    const result = await safeInvoke('delete_group', { id });
    if (result.success) {
      set((state) => ({
        groups: state.groups.filter((g) => g.id !== id),
        // Move servers from deleted group to ungrouped
        servers: state.servers.map((s) =>
          s.group_id === id ? { ...s, group_id: undefined } : s
        ),
        loading: false,
      }));
      scheduleCloudSync();
    } else {
      set({ error: result.error.message, loading: false });
      showError('Failed to Delete Group', result.error);
    }
  },

  // Utility actions
  clearError: () => {
    set({ error: null });
  },

  getServersByGroup: (groupId: string | null) => {
    const { servers } = get();
    if (groupId === null) {
      return servers.filter((s) => !s.group_id);
    }
    return servers.filter((s) => s.group_id === groupId);
  },
}));

/**
 * Helper hook to get servers organized by groups
 * Uses useMemo to avoid creating new arrays on every render
 */
export function useServersWithGroups() {
  // Subscribe to both servers and groups to trigger re-render when either changes
  const servers = useServerStore((state) => state.servers);
  const groups = useServerStore((state) => state.groups);

  // Memoize the grouped servers to avoid creating new objects on every render
  const groupedServers = useMemo(() => {
    return groups.map((group) => ({
      group,
      servers: servers.filter((s) => s.group_id === group.id),
    }));
  }, [groups, servers]);

  // Memoize ungrouped servers
  const ungroupedServers = useMemo(() => {
    return servers.filter((s) => !s.group_id);
  }, [servers]);

  return { groupedServers, ungroupedServers };
}
