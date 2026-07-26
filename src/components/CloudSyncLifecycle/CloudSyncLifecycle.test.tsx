import { cleanup, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { refreshCloudSyncEntities } from '../../stores/cloudSyncCoordinator';
import { useCloudSyncStore } from '../../stores/cloudSyncStore';
import { useServerStore } from '../../stores/serverStore';
import { useSnippetStore } from '../../stores/snippetStore';
import { CloudSyncLifecycle } from './CloudSyncLifecycle';

describe('CloudSyncLifecycle', () => {
  const refreshStatus = vi.fn().mockResolvedValue(undefined);
  const syncNow = vi.fn().mockResolvedValue(null);
  const fetchServers = vi.fn().mockResolvedValue(undefined);
  const fetchGroups = vi.fn().mockResolvedValue(undefined);
  const fetchSnippets = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.useFakeTimers();
    refreshStatus.mockClear();
    syncNow.mockClear();
    fetchServers.mockClear();
    fetchGroups.mockClear();
    fetchSnippets.mockClear();
    useCloudSyncStore.setState((state) => ({
      ...state,
      status: {
        ...state.status,
        unlocked: true,
      },
      refreshStatus,
      syncNow,
    }));
    useServerStore.setState({ fetchServers, fetchGroups });
    useSnippetStore.setState({ fetchSnippets });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it('syncs on foreground connectivity events and the foreground interval', async () => {
    render(<CloudSyncLifecycle />);
    expect(refreshStatus).toHaveBeenCalledOnce();
    await refreshCloudSyncEntities();
    expect(fetchServers).toHaveBeenCalledOnce();
    expect(fetchGroups).toHaveBeenCalledOnce();
    expect(fetchSnippets).toHaveBeenCalledOnce();

    window.dispatchEvent(new Event('online'));
    await vi.advanceTimersByTimeAsync(60_000);

    expect(syncNow).toHaveBeenCalledTimes(2);
  });
});
