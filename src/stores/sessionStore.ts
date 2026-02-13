import { create } from 'zustand';
import { safeInvoke, TauriError, sendInputBatched, fireAndForgetInvoke } from '../lib/tauri';
import { useNotificationStore } from './notificationStore';

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
 * Session type: SSH or Local Shell
 */
export type SessionType = 'ssh' | 'local';

/**
 * Session info returned from the backend
 */
export interface SessionInfo {
  id: string;
  server_id: string;
  server_name: string;
  state: 'connecting' | 'connected' | 'disconnected' | 'error';
  created_at: number;
  clients: number;
}

/**
 * Local shell session info returned from the backend
 */
export interface LocalShellSessionInfo {
  id: string;
  shellId: string;
  shellName: string;
  state: 'starting' | 'running' | 'stopped' | 'error';
  createdAt: number;
  clients: number;
}

/**
 * Connection state for a session
 */
export type SessionState = 'connecting' | 'connected' | 'disconnected' | 'error';

/**
 * Represents an active session (SSH or Local Shell)
 */
export interface Session {
  /** Unique identifier for the session */
  id: string;
  /** Server ID this session is connected to (for SSH) or shell ID (for local) */
  serverId: string;
  /** Display name from the server or shell name */
  serverName: string;
  /** Current connection state */
  state: SessionState;
  /** Timestamp when session was created */
  createdAt: number;
  /** Error message if state is 'error' */
  errorMessage?: string;
  /** Session type: SSH or Local Shell */
  sessionType: SessionType;
}

/**
 * Session store state and actions
 */
interface SessionStore {
  /** List of all sessions */
  sessions: Session[];
  /** Currently active session ID */
  activeSessionId: string | null;
  /** Loading state */
  loading: boolean;
  /** Error message */
  error: string | null;

  /** Set the active session */
  setActiveSession: (id: string | null) => void;
  /** Add a new session */
  addSession: (session: Session) => void;
  /** Remove a session by ID */
  removeSession: (id: string) => void;
  /** Update a session's properties */
  updateSession: (id: string, updates: Partial<Session>) => void;
  /** Get session by ID */
  getSession: (id: string) => Session | undefined;
  /** Get sessions for a specific server */
  getSessionsByServer: (serverId: string) => Session[];
  /** Clear all sessions */
  clearAllSessions: () => void;
  /** Clear error */
  clearError: () => void;

  // Tauri backend integration
  /** Connect to a server by name and create a session (without full SSH connection) */
  connectSession: (serverName: string) => Promise<Session | null>;
  /** Connect to a server with credentials and start SSH session */
  connectWithCredentials: (
    serverName: string,
    authType: 'password' | 'key',
    credential: string,
    passphrase?: string,
    cols?: number,
    rows?: number
  ) => Promise<Session | null>;
  /** Attach to an existing session and start receiving output */
  attachSession: (sessionId: string) => Promise<boolean>;
  /** Detach from a session */
  detachSession: (sessionId: string) => Promise<boolean>;
  /** Send input data to a session */
  sendInput: (sessionId: string, data: string) => Promise<boolean>;
  /**
   * Fast input send for performance-critical paths (typing).
   * Uses fire-and-forget with batching for minimal latency.
   * Does NOT return success/failure - use sendInput if you need confirmation.
   */
  sendInputFast: (sessionId: string, data: string) => void;
  /** Send raw bytes to a session */
  sendBytes: (sessionId: string, data: Uint8Array) => Promise<boolean>;
  /** Kill a session on the backend */
  killSession: (sessionId: string) => Promise<boolean>;
  /** Resize a session's terminal */
  resizeSession: (sessionId: string, cols: number, rows: number) => Promise<boolean>;
  /** Fetch all sessions from the backend */
  fetchSessions: () => Promise<void>;

  // Local shell session methods
  /** Create a local shell session */
  createLocalShellSession: (
    shellId?: string,
    cols?: number,
    rows?: number
  ) => Promise<Session | null>;
  /** Send input to a local shell session */
  sendLocalShellInput: (sessionId: string, data: string) => Promise<boolean>;
  /** Send input fast to a local shell session (fire-and-forget) */
  sendLocalShellInputFast: (sessionId: string, data: string) => void;
  /** Resize a local shell session */
  resizeLocalShellSession: (sessionId: string, cols: number, rows: number) => Promise<boolean>;
  /** Kill a local shell session */
  killLocalShellSession: (sessionId: string) => Promise<boolean>;
}

/**
 * Zustand store for managing SSH sessions
 */
export const useSessionStore = create<SessionStore>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  loading: false,
  error: null,

  setActiveSession: (id) => {
    set({ activeSessionId: id });
  },

  addSession: (session) => {
    set((state) => ({
      sessions: [...state.sessions, session],
      // Auto-activate the new session if no active session exists
      activeSessionId: state.activeSessionId ?? session.id,
    }));
  },

  removeSession: (id) => {
    set((state) => {
      const newSessions = state.sessions.filter((s) => s.id !== id);
      let newActiveId = state.activeSessionId;

      // If we're removing the active session, switch to another one
      if (state.activeSessionId === id) {
        // Find the session index
        const removedIndex = state.sessions.findIndex((s) => s.id === id);
        // Try to select the previous session, or the next one, or null
        if (newSessions.length > 0) {
          const newIndex = Math.min(removedIndex, newSessions.length - 1);
          newActiveId = newSessions[newIndex].id;
        } else {
          newActiveId = null;
        }
      }

      return {
        sessions: newSessions,
        activeSessionId: newActiveId,
      };
    });
  },

  updateSession: (id, updates) => {
    set((state) => ({
      sessions: state.sessions.map((s) =>
        s.id === id ? { ...s, ...updates } : s
      ),
    }));
  },

  getSession: (id) => {
    return get().sessions.find((s) => s.id === id);
  },

  getSessionsByServer: (serverId) => {
    return get().sessions.filter((s) => s.serverId === serverId);
  },

  clearAllSessions: () => {
    set({ sessions: [], activeSessionId: null });
  },

  clearError: () => {
    set({ error: null });
  },

  // Tauri backend integration methods
  connectSession: async (serverName: string) => {
    set({ loading: true, error: null });

    const result = await safeInvoke<SessionInfo>('session_create', {
      request: {
        serverName: serverName,
      },
    });

    if (result.success) {
      // Convert backend SessionInfo to frontend Session
      const session: Session = {
        id: result.data.id,
        serverId: result.data.server_id,
        serverName: result.data.server_name,
        state: result.data.state,
        createdAt: result.data.created_at * 1000, // Convert seconds to ms
        sessionType: 'ssh',
      };

      set((state) => ({
        sessions: [...state.sessions, session],
        activeSessionId: state.activeSessionId ?? session.id,
        loading: false,
      }));

      return session;
    } else {
      set({
        error: result.error.message,
        loading: false,
      });
      showError('Failed to Create Session', result.error);
      return null;
    }
  },

  connectWithCredentials: async (
    serverName: string,
    authType: 'password' | 'key',
    credential: string,
    passphrase?: string,
    cols?: number,
    rows?: number
  ) => {
    console.log('[sessionStore] connectWithCredentials called:', {
      serverName,
      authType,
      hasCredential: !!credential,
      hasPassphrase: !!passphrase,
      cols: cols ?? 80,
      rows: rows ?? 24,
    });

    set({ loading: true, error: null });

    console.log('[sessionStore] Invoking session_connect command...');
    const result = await safeInvoke<SessionInfo>('session_connect', {
      request: {
        serverName: serverName,
        authType: authType,
        credential,
        passphrase: passphrase ?? null,
        cols: cols ?? 80,
        rows: rows ?? 24,
      },
    });

    console.log('[sessionStore] session_connect result:', result);

    if (result.success) {
      const session: Session = {
        id: result.data.id,
        serverId: result.data.server_id,
        serverName: result.data.server_name,
        state: result.data.state,
        createdAt: result.data.created_at * 1000,
        sessionType: 'ssh',
      };

      console.log('[sessionStore] Session created:', session);

      set((state) => ({
        sessions: [...state.sessions, session],
        activeSessionId: state.activeSessionId ?? session.id,
        loading: false,
      }));

      return session;
    } else {
      console.error('[sessionStore] session_connect failed:', result.error.message);
      set({
        error: result.error.message,
        loading: false,
      });
      showError('Connection Failed', result.error);
      return null;
    }
  },

  attachSession: async (sessionId: string) => {
    console.log('[sessionStore] attachSession called for:', sessionId);
    console.log('[sessionStore] Invoking session_attach command...');
    const result = await safeInvoke<SessionInfo>('session_attach', {
      request: {
        sessionId: sessionId,
      },
    });

    console.log('[sessionStore] session_attach result:', result);

    if (result.success) {
      // Update session state if needed
      const { updateSession } = get();
      updateSession(sessionId, { state: result.data.state });
      console.log('[sessionStore] Session state updated to:', result.data.state);
      return true;
    }
    console.warn('[sessionStore] session_attach failed:', result.error.message);
    showError('Failed to Attach Session', result.error);
    return false;
  },

  detachSession: async (sessionId: string) => {
    const result = await safeInvoke('session_detach', {
      request: {
        sessionId: sessionId,
      },
    });
    if (!result.success) {
      console.warn('Failed to detach from session:', result.error.message);
      // Don't show toast for detach failures - usually not critical
    }
    return result.success;
  },

  sendInput: async (sessionId: string, data: string) => {
    const result = await safeInvoke('session_send_input', {
      request: {
        sessionId: sessionId,
        data,
      },
    });
    if (!result.success) {
      console.warn('Failed to send input:', result.error.message);
      // Only show toast for non-Tauri-unavailable errors
      if (!result.error.isTauriUnavailable) {
        showError('Input Failed', result.error);
      }
    }
    return result.success;
  },

  // PERFORMANCE OPTIMIZED: Fire-and-forget with batching for instant input
  sendInputFast: (sessionId: string, data: string) => {
    sendInputBatched(sessionId, data);
  },

  sendBytes: async (sessionId: string, data: Uint8Array) => {
    const result = await safeInvoke('session_send_bytes', {
      request: {
        sessionId: sessionId,
        data: Array.from(data),
      },
    });
    if (!result.success) {
      console.warn('Failed to send bytes:', result.error.message);
      // Don't show toast for every byte send failure - too noisy
    }
    return result.success;
  },

  killSession: async (sessionId: string) => {
    const result = await safeInvoke('session_kill', {
      request: {
        sessionId: sessionId,
      },
    });

    if (result.success) {
      // Remove the session from the local store
      get().removeSession(sessionId);
      return true;
    }
    console.warn('Failed to kill session:', result.error.message);
    showError('Failed to Close Session', result.error);
    return false;
  },

  resizeSession: async (sessionId: string, cols: number, rows: number) => {
    const result = await safeInvoke('session_resize', {
      request: {
        sessionId: sessionId,
        cols,
        rows,
      },
    });
    if (!result.success) {
      console.warn('Failed to resize session:', result.error.message);
      // Don't show toast for resize failures - usually not critical
    }
    return result.success;
  },

  fetchSessions: async () => {
    set({ loading: true, error: null });

    const result = await safeInvoke<SessionInfo[]>('session_list');

    if (result.success) {
      const sessions: Session[] = result.data.map((info) => ({
        id: info.id,
        serverId: info.server_id,
        serverName: info.server_name,
        state: info.state,
        createdAt: info.created_at * 1000,
        sessionType: 'ssh' as const,
      }));

      set({ sessions, loading: false });
    } else {
      // Set empty state but track the error
      const errorMsg = result.error.isTauriUnavailable
        ? 'Running in browser mode'
        : result.error.message;
      set({ sessions: [], loading: false, error: errorMsg });
      // Don't show toast for initial fetch failures when Tauri unavailable
      if (!result.error.isTauriUnavailable) {
        showError('Failed to Load Sessions', result.error);
      }
    }
  },

  // Local shell session methods
  createLocalShellSession: async (shellId?: string, cols?: number, rows?: number) => {
    set({ loading: true, error: null });

    const result = await safeInvoke<LocalShellSessionInfo>('local_shell_create', {
      request: {
        shellId: shellId ?? null,
        cols: cols ?? 80,
        rows: rows ?? 24,
      },
    });

    if (result.success) {
      // Map local shell state to session state
      const stateMap: Record<string, SessionState> = {
        starting: 'connecting',
        running: 'connected',
        stopped: 'disconnected',
        error: 'error',
      };

      const session: Session = {
        id: result.data.id,
        serverId: result.data.shellId,
        serverName: result.data.shellName,
        state: stateMap[result.data.state] || 'connecting',
        createdAt: result.data.createdAt * 1000,
        sessionType: 'local',
      };

      set((state) => ({
        sessions: [...state.sessions, session],
        activeSessionId: state.activeSessionId ?? session.id,
        loading: false,
      }));

      return session;
    } else {
      set({
        error: result.error.message,
        loading: false,
      });
      showError('Failed to Create Local Shell', result.error);
      return null;
    }
  },

  sendLocalShellInput: async (sessionId: string, data: string) => {
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

  sendLocalShellInputFast: (sessionId: string, data: string) => {
    fireAndForgetInvoke('local_shell_send_input', {
      request: {
        sessionId,
        data,
      },
    });
  },

  resizeLocalShellSession: async (sessionId: string, cols: number, rows: number) => {
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

  killLocalShellSession: async (sessionId: string) => {
    const result = await safeInvoke('local_shell_kill', {
      request: {
        sessionId,
      },
    });

    if (result.success) {
      get().removeSession(sessionId);
      return true;
    }
    showError('Failed to Close Local Shell', result.error);
    return false;
  },
}));

/**
 * Helper function to generate a unique session ID
 */
export function generateSessionId(): string {
  return `session-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

/**
 * Helper function to create a new session object
 */
export function createSession(
  serverId: string,
  serverName: string,
  sessionType: SessionType = 'ssh'
): Session {
  return {
    id: generateSessionId(),
    serverId,
    serverName,
    state: 'connecting',
    createdAt: Date.now(),
    sessionType,
  };
}
