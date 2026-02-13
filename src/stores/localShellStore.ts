import { create } from 'zustand';
import { safeInvoke, TauriError, fireAndForgetInvoke } from '../lib/tauri';
import { useNotificationStore } from './notificationStore';

/**
 * Helper to show error notification
 */
function showError(title: string, error: TauriError): void {
  const { error: notifyError } = useNotificationStore.getState();
  if (!error.isTauriUnavailable) {
    notifyError(title, error.message);
  }
}

/**
 * Shell type categories
 */
export type ShellType = 'power_shell' | 'cmd' | 'bash' | 'zsh' | 'fish' | 'sh' | 'other';

/**
 * Information about an available shell
 */
export interface ShellInfo {
  /** Unique identifier for the shell (e.g., "powershell", "cmd", "bash") */
  id: string;
  /** Display name for the shell */
  name: string;
  /** Full path to the shell executable */
  path: string;
  /** Shell type category */
  shellType: ShellType;
  /** Whether this is the system default shell */
  isDefault: boolean;
}

/**
 * State of a local shell session
 */
export type LocalShellState = 'starting' | 'running' | 'stopped' | 'error';

/**
 * Information about a local shell session
 */
export interface LocalShellInfo {
  id: string;
  shellId: string;
  shellName: string;
  state: LocalShellState;
  createdAt: number;
  clients: number;
}

/**
 * Request to create a local shell session
 */
export interface CreateLocalShellRequest {
  shellId?: string;
  cols?: number;
  rows?: number;
}

/**
 * Local shell store state and actions
 */
interface LocalShellStore {
  /** Available shells on the system */
  availableShells: ShellInfo[];
  /** Default shell (if detected) */
  defaultShell: ShellInfo | null;
  /** Active local shell sessions */
  sessions: LocalShellInfo[];
  /** Loading state */
  loading: boolean;
  /** Error message */
  error: string | null;
  /** Last selected shell preference */
  lastSelectedShellId: string | null;

  /** Fetch available shells */
  fetchAvailableShells: () => Promise<void>;
  /** Get default shell */
  fetchDefaultShell: () => Promise<void>;
  /** Fetch all local shell sessions */
  fetchSessions: () => Promise<void>;
  /** Create a new local shell session */
  createSession: (shellId?: string, cols?: number, rows?: number) => Promise<LocalShellInfo | null>;
  /** Send input to a session */
  sendInput: (sessionId: string, data: string) => Promise<boolean>;
  /** Send input fast (fire-and-forget) */
  sendInputFast: (sessionId: string, data: string) => void;
  /** Send raw bytes to a session */
  sendBytes: (sessionId: string, data: Uint8Array) => Promise<boolean>;
  /** Resize a session */
  resizeSession: (sessionId: string, cols: number, rows: number) => Promise<boolean>;
  /** Kill a session */
  killSession: (sessionId: string) => Promise<boolean>;
  /** Kill all sessions */
  killAllSessions: () => Promise<boolean>;
  /** Set last selected shell preference */
  setLastSelectedShell: (shellId: string) => void;
  /** Clear error */
  clearError: () => void;
}

/**
 * Zustand store for managing local shell sessions
 */
export const useLocalShellStore = create<LocalShellStore>((set) => ({
  availableShells: [],
  defaultShell: null,
  sessions: [],
  loading: false,
  error: null,
  lastSelectedShellId: localStorage.getItem('lastSelectedShellId'),

  fetchAvailableShells: async () => {
    const result = await safeInvoke<ShellInfo[]>('local_shell_list_shells');
    if (result.success) {
      set({ availableShells: result.data });
    } else {
      console.warn('Failed to fetch available shells:', result.error.message);
    }
  },

  fetchDefaultShell: async () => {
    const result = await safeInvoke<ShellInfo | null>('local_shell_get_default');
    if (result.success) {
      set({ defaultShell: result.data });
    } else {
      console.warn('Failed to fetch default shell:', result.error.message);
    }
  },

  fetchSessions: async () => {
    set({ loading: true });
    const result = await safeInvoke<LocalShellInfo[]>('local_shell_list_sessions');
    if (result.success) {
      set({ sessions: result.data, loading: false });
    } else {
      set({ loading: false, error: result.error.message });
    }
  },

  createSession: async (shellId?: string, cols?: number, rows?: number) => {
    set({ loading: true, error: null });

    const result = await safeInvoke<LocalShellInfo>('local_shell_create', {
      request: {
        shellId: shellId ?? null,
        cols: cols ?? 80,
        rows: rows ?? 24,
      },
    });

    if (result.success) {
      set((state) => ({
        sessions: [...state.sessions, result.data],
        loading: false,
      }));
      return result.data;
    } else {
      set({
        error: result.error.message,
        loading: false,
      });
      showError('Failed to Create Local Shell', result.error);
      return null;
    }
  },

  sendInput: async (sessionId: string, data: string) => {
    const result = await safeInvoke('local_shell_send_input', {
      request: {
        sessionId,
        data,
      },
    });
    if (!result.success) {
      console.warn('Failed to send input to local shell:', result.error.message);
    }
    return result.success;
  },

  sendInputFast: (sessionId: string, data: string) => {
    fireAndForgetInvoke('local_shell_send_input', {
      request: {
        sessionId,
        data,
      },
    });
  },

  sendBytes: async (sessionId: string, data: Uint8Array) => {
    const result = await safeInvoke('local_shell_send_bytes', {
      request: {
        sessionId,
        data: Array.from(data),
      },
    });
    if (!result.success) {
      console.warn('Failed to send bytes to local shell:', result.error.message);
    }
    return result.success;
  },

  resizeSession: async (sessionId: string, cols: number, rows: number) => {
    const result = await safeInvoke('local_shell_resize', {
      request: {
        sessionId,
        cols,
        rows,
      },
    });
    if (!result.success) {
      console.warn('Failed to resize local shell:', result.error.message);
    }
    return result.success;
  },

  killSession: async (sessionId: string) => {
    const result = await safeInvoke('local_shell_kill', {
      request: {
        sessionId,
      },
    });

    if (result.success) {
      set((state) => ({
        sessions: state.sessions.filter((s) => s.id !== sessionId),
      }));
      return true;
    }
    showError('Failed to Close Local Shell', result.error);
    return false;
  },

  killAllSessions: async () => {
    const result = await safeInvoke('local_shell_kill_all');
    if (result.success) {
      set({ sessions: [] });
      return true;
    }
    showError('Failed to Close All Local Shells', result.error);
    return false;
  },

  setLastSelectedShell: (shellId: string) => {
    localStorage.setItem('lastSelectedShellId', shellId);
    set({ lastSelectedShellId: shellId });
  },

  clearError: () => {
    set({ error: null });
  },
}));

/**
 * Hook to get available shells with the default shell marked
 */
export function useAvailableShells() {
  const availableShells = useLocalShellStore((state) => state.availableShells);
  const defaultShell = useLocalShellStore((state) => state.defaultShell);
  const lastSelectedShellId = useLocalShellStore((state) => state.lastSelectedShellId);

  // Sort shells with default first, then by name
  const sortedShells = [...availableShells].sort((a, b) => {
    if (a.isDefault && !b.isDefault) return -1;
    if (!a.isDefault && b.isDefault) return 1;
    return a.name.localeCompare(b.name);
  });

  // Determine which shell to suggest (last selected or default)
  const suggestedShell = lastSelectedShellId
    ? availableShells.find((s) => s.id === lastSelectedShellId) || defaultShell
    : defaultShell;

  return {
    shells: sortedShells,
    defaultShell,
    suggestedShell,
  };
}
