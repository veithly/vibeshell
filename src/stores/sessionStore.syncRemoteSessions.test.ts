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

describe('sessionStore.syncRemoteSessions', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset();
    useSessionStore.setState({
      sessions: [],
      activeSessionId: null,
      loading: false,
      error: null,
    });
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

    safeInvokeMock.mockResolvedValue({
      success: true,
      data: [] satisfies SessionInfo[],
    });

    await useSessionStore.getState().syncRemoteSessions();

    expect(useSessionStore.getState().sessions).toEqual([]);
    expect(useSessionStore.getState().activeSessionId).toBeNull();
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

    safeInvokeMock.mockResolvedValue({
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
    });

    const listener = vi.fn();
    const unsubscribe = useSessionStore.subscribe(listener);

    await useSessionStore.getState().syncRemoteSessions();

    expect(listener).not.toHaveBeenCalled();
    unsubscribe();
  });
});
