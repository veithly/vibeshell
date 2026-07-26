import { beforeEach, describe, expect, it, vi } from 'vitest';
import { registerCloudSyncEntityRefresh } from './cloudSyncCoordinator';
import { scheduleCloudSync, useCloudSyncStore } from './cloudSyncStore';

const safeInvokeMock = vi.hoisted(() => vi.fn());
const refreshEntitiesMock = vi.fn().mockResolvedValue(undefined);

vi.mock('../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

const unlockedStatus = {
  unlocked: true,
  syncing: false,
  provider: 'webdav' as const,
  endpoint: 'https://dav.example.com/vibeshell-sync.json',
  vaultId: 'vault-1',
  pendingChanges: 2,
  conflicts: 0,
  lastSuccessAt: null,
  lastError: null,
};

describe('cloudSyncStore', () => {
  beforeEach(() => {
    vi.useRealTimers();
    safeInvokeMock.mockReset();
    refreshEntitiesMock.mockClear();
    registerCloudSyncEntityRefresh(refreshEntitiesMock);
    useCloudSyncStore.setState({
      status: {
        unlocked: false,
        syncing: false,
        provider: null,
        endpoint: null,
        vaultId: null,
        pendingChanges: 0,
        conflicts: 0,
        lastSuccessAt: null,
        lastError: null,
      },
      pairingInfo: null,
      report: null,
      fileReport: null,
      loading: false,
      error: null,
    });
  });

  it('debounces local mutations only while the vault is unlocked', async () => {
    vi.useFakeTimers();
    safeInvokeMock
      .mockResolvedValueOnce({
        success: true,
        data: {
          uploaded: 1,
          downloaded: 0,
          applied: 0,
          ignored: 0,
          conflicts: 0,
          pendingChanges: 0,
          cursor: '1',
        },
      })
      .mockResolvedValueOnce({ success: true, data: unlockedStatus });

    scheduleCloudSync(25);
    await vi.advanceTimersByTimeAsync(25);
    expect(safeInvokeMock).not.toHaveBeenCalled();

    useCloudSyncStore.setState({ status: unlockedStatus });
    scheduleCloudSync(25);
    scheduleCloudSync(25);
    await vi.advanceTimersByTimeAsync(25);

    expect(safeInvokeMock).toHaveBeenCalledTimes(2);
    expect(safeInvokeMock).toHaveBeenNthCalledWith(1, 'cloud_sync_now');
  });

  it('keeps the returned pairing code in memory after vault creation', async () => {
    safeInvokeMock
      .mockResolvedValueOnce({
        success: true,
        data: {
          provider: 'webdav',
          endpoint: 'https://dav.example.com/vibeshell-sync.json',
          vaultId: 'vault-1',
          pairingCode: 'vibeshell-sync-v2.secret',
        },
      })
      .mockResolvedValueOnce({ success: true, data: unlockedStatus })
      .mockResolvedValueOnce({
        success: true,
        data: {
          uploaded: 0,
          downloaded: 0,
          applied: 0,
          ignored: 0,
          conflicts: 0,
          pendingChanges: 0,
          cursor: '0',
        },
      })
      .mockResolvedValueOnce({
        success: true,
        data: { ...unlockedStatus, pendingChanges: 0, lastSuccessAt: 123 },
      });

    await useCloudSyncStore.getState().createVault({
      provider: 'webdav',
      endpoint: 'https://dav.example.com/vibeshell-sync.json',
      username: '',
      password: '',
    });

    expect(useCloudSyncStore.getState().pairingInfo?.pairingCode).toBe(
      'vibeshell-sync-v2.secret'
    );
    expect(safeInvokeMock).toHaveBeenNthCalledWith(1, 'cloud_sync_create_vault', {
      request: {
        provider: 'webdav',
        endpoint: 'https://dav.example.com/vibeshell-sync.json',
        username: '',
        password: '',
      },
    });
    expect(safeInvokeMock).toHaveBeenNthCalledWith(3, 'cloud_sync_now');
  });

  it('refreshes pending state after a successful manual sync', async () => {
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'cloud_sync_now') {
        return Promise.resolve({
          success: true,
          data: {
            uploaded: 2,
            downloaded: 3,
            applied: 1,
            ignored: 2,
            conflicts: 0,
            pendingChanges: 0,
            cursor: '4',
          },
        });
      }
      if (command === 'cloud_sync_status') {
        return Promise.resolve({
          success: true,
          data: { ...unlockedStatus, pendingChanges: 0, lastSuccessAt: 123 },
        });
      }
      return Promise.resolve({ success: true, data: [] });
    });

    const report = await useCloudSyncStore.getState().syncNow();

    expect(report?.cursor).toBe('4');
    expect(useCloudSyncStore.getState().status.pendingChanges).toBe(0);
    expect(useCloudSyncStore.getState().status.lastSuccessAt).toBe(123);
    expect(refreshEntitiesMock).toHaveBeenCalledOnce();
  });

  it('refreshes workspace entities after importing a portable file', async () => {
    safeInvokeMock.mockResolvedValueOnce({
      success: true,
      data: {
        operation: 'import',
        path: '/tmp/workspace.json',
        exported: 0,
        imported: 2,
        applied: 2,
        ignored: 0,
        conflicts: 0,
      },
    });

    const report = await useCloudSyncStore.getState().importFile();

    expect(report?.applied).toBe(2);
    expect(refreshEntitiesMock).toHaveBeenCalledOnce();
    expect(safeInvokeMock).toHaveBeenCalledWith('cloud_sync_import_file');
  });
});
