import { create } from 'zustand';
import { safeInvoke, TauriError, sendInputBatched } from '../lib/tauri';
import { useNotificationStore } from './notificationStore';
import { useFileWorkspaceStore } from './fileWorkspaceStore';
import type { CodingAgentLaunchRequest } from '../types/codingAgent';

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

function mapSessionInfo(info: SessionInfo): Session {
  return {
    id: info.id,
    serverId: info.server_id,
    serverName: info.server_name,
    state: info.state,
    createdAt: info.created_at * 1000,
    sessionType: 'ssh',
  };
}

const localStateMap: Record<LocalShellSessionInfo['state'], SessionState> = {
  starting: 'connecting',
  running: 'connected',
  stopped: 'disconnected',
  error: 'error',
};

function mapLocalSessionInfo(info: LocalShellSessionInfo): Session {
  return {
    id: info.id,
    serverId: info.shellId,
    serverName: info.shellName,
    state: localStateMap[info.state],
    createdAt: info.createdAt * 1000,
    sessionType: 'local',
    purpose: info.agentId ? 'coding_agent' : 'shell',
    agentId: info.agentId ?? undefined,
    cwd: info.cwd ?? undefined,
  };
}

function upsertSession(sessions: Session[], session: Session): Session[] {
  const index = sessions.findIndex((existing) => existing.id === session.id);
  if (index === -1) {
    return [...sessions, session];
  }

  return sessions.map((existing, existingIndex) =>
    existingIndex === index ? { ...existing, ...session } : existing
  );
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
  cwd?: string | null;
  agentId?: string | null;
  state: 'starting' | 'running' | 'stopped' | 'error';
  createdAt: number;
  clients: number;
}

/**
 * Connection state for a session
 */
export type SessionState = 'connecting' | 'connected' | 'disconnected' | 'error';
export type SessionPurpose = 'shell' | 'coding_agent';

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
  /** What the local PTY is being used for. SSH sessions omit this. */
  purpose?: SessionPurpose;
  /** Coding agent adapter ID for embedded agent sessions. */
  agentId?: string;
  /** Workspace selected when the coding agent was launched. */
  cwd?: string;
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
    rows?: number,
    forceNew?: boolean
  ) => Promise<Session | null>;
  /** Attach to an existing session and start receiving output */
  attachSession: (sessionId: string) => Promise<boolean>;
  /** Attach to a local shell session (replays buffered output) */
  attachLocalShellSession: (sessionId: string) => Promise<boolean>;
  /** Detach a terminal view from a local PTY session. */
  detachLocalShellSession: (sessionId: string) => Promise<boolean>;
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
  /** Sync remote (CLI-created) sessions into the local store without losing local shell sessions */
  syncRemoteSessions: () => Promise<void>;

  // Local shell session methods
  /** Create a local shell session */
  createLocalShellSession: (
    shellId?: string,
    cols?: number,
    rows?: number
  ) => Promise<Session | null>;
  /** Launch a local coding agent inside a PTY-backed session. */
  launchCodingAgentSession: (request: CodingAgentLaunchRequest) => Promise<Session | null>;
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
    useFileWorkspaceStore.getState().closeTabsForSession(id);
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
    useFileWorkspaceStore.getState().retainTabsForSessions([]);
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
      const session = mapSessionInfo(result.data);

      set((state) => ({
        sessions: upsertSession(state.sessions, session),
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
    rows?: number,
    forceNew = false
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
        // An empty passphrase means "unencrypted key" — send null so the
        // backend never tries to decrypt the key with an empty string.
        passphrase: passphrase ? passphrase : null,
        cols: cols ?? 80,
        rows: rows ?? 24,
        forceNew,
      },
    });

    console.log('[sessionStore] session_connect result:', result);

    if (result.success) {
      const session = mapSessionInfo(result.data);

      console.log('[sessionStore] Session created:', session);

      set((state) => ({
        sessions: upsertSession(state.sessions, session),
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

  attachLocalShellSession: async (sessionId: string) => {
    // local_shell_attach replays the buffered output (recent history +
    // initial prompt) via session-output events and spawns a forwarder for
    // subsequent live output. The listener must already be registered before
    // this call so replayed chunks are not lost.
    const result = await safeInvoke('local_shell_attach', {
      request: {
        sessionId: sessionId,
      },
    });
    if (!result.success) {
      console.warn('[sessionStore] local_shell_attach failed:', result.error.message);
    }
    return result.success;
  },

  detachLocalShellSession: async (sessionId: string) => {
    const result = await safeInvoke('local_shell_detach', {
      request: { sessionId },
    });
    if (!result.success) {
      console.warn('[sessionStore] local_shell_detach failed:', result.error.message);
    }
    return result.success;
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
      const sessions: Session[] = result.data.map(mapSessionInfo);

      useFileWorkspaceStore
        .getState()
        .retainTabsForSessions(sessions.map((session) => session.id));

      set((state) => {
        const activeSessionStillExists = state.activeSessionId
          ? sessions.some((session) => session.id === state.activeSessionId)
          : false;

        return {
          sessions,
          loading: false,
          activeSessionId: activeSessionStillExists
            ? state.activeSessionId
            : (sessions[0]?.id ?? null),
        };
      });
    } else {
      // Set empty state but track the error
      const errorMsg = result.error.isTauriUnavailable
        ? 'Running in browser mode'
        : result.error.message;
      useFileWorkspaceStore.getState().retainTabsForSessions([]);
      set({ sessions: [], loading: false, error: errorMsg });
      // Don't show toast for initial fetch failures when Tauri unavailable
      if (!result.error.isTauriUnavailable) {
        showError('Failed to Load Sessions', result.error);
      }
    }
  },

  syncRemoteSessions: async () => {
    const [result, localResult] = await Promise.all([
      safeInvoke<SessionInfo[]>('session_list'),
      safeInvoke<LocalShellSessionInfo[]>('local_shell_list_sessions'),
    ]);
    if (!result.success) return;

    const backendSessions = result.data;
    const backendById = new Map(backendSessions.map((session) => [session.id, session]));
    const backendLocalById = localResult.success
      ? new Map(localResult.data.map((session) => [session.id, session]))
      : null;

    // Merge against the latest state after the async IPC call so a local shell
    // created while session_list is in flight cannot be overwritten.
    set((state) => {
      const knownSshIds = new Set(
        state.sessions.filter((session) => session.sessionType === 'ssh').map((session) => session.id)
      );
      const knownLocalIds = new Set(
        state.sessions.filter((session) => session.sessionType === 'local').map((session) => session.id)
      );
      let changed = false;
      const merged = state.sessions.flatMap((session) => {
        if (session.sessionType === 'local') {
          const backend = backendLocalById?.get(session.id);
          if (!backend) return [session];
          const mapped = mapLocalSessionInfo(backend);
          if (
            mapped.state !== session.state
            || mapped.serverName !== session.serverName
            || mapped.cwd !== session.cwd
            || mapped.agentId !== session.agentId
          ) {
            changed = true;
            return [{ ...session, ...mapped }];
          }
          return [session];
        }
        const backend = backendById.get(session.id);
        if (!backend) {
          changed = true;
          return [];
        }
        if (backend.state !== session.state) {
          changed = true;
          return [{ ...session, state: backend.state }];
        }
        return [session];
      });

      const newSessions = backendSessions
        .filter((info) => !knownSshIds.has(info.id))
        .map(mapSessionInfo);
      const newLocalSessions = localResult.success
        ? localResult.data.filter((info) => !knownLocalIds.has(info.id)).map(mapLocalSessionInfo)
        : [];
      if (newSessions.length > 0) {
        changed = true;
        merged.push(...newSessions);
      }
      if (newLocalSessions.length > 0) {
        changed = true;
        merged.push(...newLocalSessions);
      }

      const nextActiveSessionId =
        state.activeSessionId && merged.some((session) => session.id === state.activeSessionId)
          ? state.activeSessionId
          : (merged[0]?.id ?? null);

      if (!changed && nextActiveSessionId === state.activeSessionId) return state;
      return { sessions: merged, activeSessionId: nextActiveSessionId };
    });
    useFileWorkspaceStore
      .getState()
      .retainTabsForSessions(get().sessions.map((session) => session.id));
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
      const session = mapLocalSessionInfo(result.data);

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

  launchCodingAgentSession: async (request: CodingAgentLaunchRequest) => {
    set({ loading: true, error: null });
    const result = await safeInvoke<LocalShellSessionInfo>('coding_agent_launch', { request });

    if (!result.success) {
      set({ loading: false, error: result.error.message });
      showError('Failed to Start Coding Agent', result.error);
      return null;
    }

    const session = mapLocalSessionInfo(result.data);
    set((state) => ({
      sessions: upsertSession(state.sessions, session),
      activeSessionId: session.id,
      loading: false,
    }));
    return session;
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
    sendInputBatched(sessionId, data, 'local_shell_send_input');
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
