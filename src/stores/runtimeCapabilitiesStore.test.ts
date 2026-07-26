import { beforeEach, describe, expect, it, vi } from 'vitest';

const { safeInvokeMock } = vi.hoisted(() => ({
  safeInvokeMock: vi.fn(),
}));

vi.mock('../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

import {
  fetchRuntimeCapabilities,
  useRuntimeCapabilitiesStore,
  WEB_RUNTIME_CAPABILITIES,
} from './runtimeCapabilitiesStore';

const desktopCapabilities = {
  platform: 'linux',
  isMobile: false,
  windowControls: true,
  localShell: true,
  agentGateway: true,
  desktopUpdater: true,
  cliIpc: true,
  directoryTransfer: true,
  backgroundTunnels: true,
} as const;

describe('runtimeCapabilitiesStore', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset();
    useRuntimeCapabilitiesStore.setState({
      capabilities: WEB_RUNTIME_CAPABILITIES,
      status: 'idle',
    });
  });

  it('loads the backend capability contract once for concurrent consumers', async () => {
    safeInvokeMock.mockResolvedValue({ success: true, data: desktopCapabilities });

    const first = useRuntimeCapabilitiesStore.getState().load();
    const second = useRuntimeCapabilitiesStore.getState().load();

    await expect(first).resolves.toEqual(desktopCapabilities);
    await expect(second).resolves.toEqual(desktopCapabilities);
    expect(safeInvokeMock).toHaveBeenCalledOnce();
    expect(safeInvokeMock).toHaveBeenCalledWith('get_runtime_capabilities');
    expect(useRuntimeCapabilitiesStore.getState()).toMatchObject({
      capabilities: desktopCapabilities,
      status: 'ready',
    });
  });

  it('uses a conservative fallback when the backend is unavailable', async () => {
    safeInvokeMock.mockResolvedValue({ success: false, error: new Error('not in Tauri') });

    await expect(fetchRuntimeCapabilities()).resolves.toBe(WEB_RUNTIME_CAPABILITIES);
    expect(Object.values(WEB_RUNTIME_CAPABILITIES).filter((value) => value === true)).toHaveLength(0);
  });
});
