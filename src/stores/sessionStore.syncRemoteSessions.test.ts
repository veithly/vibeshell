import { beforeEach, describe, expect, it, vi } from 'vitest';

const { safeInvokeMock } = vi.hoisted(() => ({
  safeInvokeMock: vi.fn(),
}));

vi.mock('../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
  sendInputBatched: vi.fn(),
  fireAndForgetInvoke: vi.fn(),
  TauriError: class TauriError extends Error {},
}));

vi.mock('./notificationStore', () => ({
  useNotificationStore: {
    getState: () => ({
      error: vi.fn(),
    }),
  },
}));

import { useSessionStore, type SessionInfo } from './sessionStore';
import { useFileWorkspaceStore } from './fileWorkspaceStore';

const openFileForSession = (sessionId: string) => {
  useFileWorkspaceStore.getState().openFile({
    sessionId,
    path: `/tmp/${sessionId}.txt`,
    name: `${sessionId}.txt`,
    size: 1,
  });
};

describe('sessionStore.syncRemoteSessions', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset();
    useSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      loading: false,
      error: null,
    });
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
  });

  it('clears file tabs when all sessions are cleared', () => {
    openFileForSession('ssh-session');

    useSessionStore.getState().clearAllSessions();

    expect(useFileWorkspaceStore.getState().tabs).toEqual([]);
    expect(useFileWorkspaceStore.getState().activeTabId).toBeNull();
  });

  it('retires file tabs for sessions omitted by a full backend refresh', async () => {
    openFileForSession('stale-session');
    openFileForSession('live-session');
    safeInvokeMock.mockResolvedValue({
      success: true,
      data: [
        {
          id: 'live-session',
          server_id: 'server-1',
          server_name: 'prod',
          state: 'connected',
          created_at: 1,
          clients: 1,
        },
      ] satisfies SessionInfo[],
    });

    await useSessionStore.getState().fetchSessions();

    expect(useFileWorkspaceStore.getState().tabs.map((tab) => tab.sessionId)).toEqual([
      'live-session',
    ]);
  });

  it('clears file tabs when a full backend refresh fails and clears sessions', async () => {
    openFileForSession('stale-session');
    safeInvokeMock.mockResolvedValue({
      success: false,
      error: { message: 'offline', isTauriUnavailable: true },
    });

    await useSessionStore.getState().fetchSessions();

    expect(useFileWorkspaceStore.getState().tabs).toEqual([]);
  });

  it('removes ssh tabs that no longer exist in the backend instead of leaving disconnected zombies', async () => {
    useSessionStore.setState({
      sessions: [
        {
          id: 'cli-ssh-session',
          serverId: 'server-1',
          serverName: 'prod',
          state: 'connected',
          createdAt: 1,
          sessionType: 'ssh',
        },
      ],
      activeSessionId: 'cli-ssh-session',
    });
    openFileForSession('cli-ssh-session');

    safeInvokeMock.mockResolvedValue({
      success: true,
      data: [] satisfies SessionInfo[],
    });

    await useSessionStore.getState().syncRemoteSessions();

    expect(useSessionStore.getState().sessions).toEqual([]);
    expect(useSessionStore.getState().activeSessionId).toBeNull();
    expect(useFileWorkspaceStore.getState().tabs).toEqual([]);
  });

  it('keeps local shell tabs while removing vanished ssh sessions', async () => {
    useSessionStore.setState({
      sessions: [
        {
          id: 'local-shell',
          serverId: 'pwsh',
          serverName: 'PowerShell',
          state: 'connected',
          createdAt: 1,
          sessionType: 'local',
        },
        {
          id: 'cli-ssh-session',
          serverId: 'server-1',
          serverName: 'prod',
          state: 'connected',
          createdAt: 2,
          sessionType: 'ssh',
        },
      ],
      activeSessionId: 'cli-ssh-session',
    });
    openFileForSession('local-shell');
    openFileForSession('cli-ssh-session');

    safeInvokeMock.mockResolvedValue({
      success: true,
      data: [] satisfies SessionInfo[],
    });

    await useSessionStore.getState().syncRemoteSessions();

    expect(useSessionStore.getState().sessions).toEqual([
      {
        id: 'local-shell',
        serverId: 'pwsh',
        serverName: 'PowerShell',
        state: 'connected',
        createdAt: 1,
        sessionType: 'local',
      },
    ]);
    expect(useSessionStore.getState().activeSessionId).toBe('local-shell');
    expect(useFileWorkspaceStore.getState().tabs.map((tab) => tab.sessionId)).toEqual([
      'local-shell',
    ]);
  });

  it('does not update store state when backend sessions are unchanged', async () => {
    useSessionStore.setState({
      sessions: [
        {
          id: 'ssh-session',
          serverId: 'server-1',
          serverName: 'prod',
          state: 'connected',
          createdAt: 1000,
          sessionType: 'ssh',
        },
      ],
      activeSessionId: 'ssh-session',
    });

    safeInvokeMock.mockImplementation(async (command: string) => {
      if (command === 'local_shell_list_sessions') {
        return { success: true, data: [] };
      }
      return {
        success: true,
        data: [
          {
            id: 'ssh-session',
            server_id: 'server-1',
            server_name: 'prod',
            state: 'connected',
            created_at: 1,
            clients: 1,
          },
        ] satisfies SessionInfo[],
      };
    });

    const listener = vi.fn();
    const unsubscribe = useSessionStore.subscribe(listener);

    await useSessionStore.getState().syncRemoteSessions();

    expect(listener).not.toHaveBeenCalled();
    unsubscribe();
  });
});
