import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';
import type { Server } from '../../stores/serverStore';
import { ConnectDialog } from './ConnectDialog';

const safeInvokeMock = vi.hoisted(() => vi.fn());

vi.mock('../../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

const keyServer: Server = {
  id: 'mobile-key-server',
  name: 'Mobile host',
  host: 'example.test',
  port: 22,
  username: 'root',
  auth_type: 'key',
  tags: [],
  created_at: 1,
  updated_at: 1,
};

describe('ConnectDialog mobile key authentication', () => {
  beforeEach(() => {
    safeInvokeMock.mockReset().mockResolvedValue({ success: true, data: null });
    useRuntimeCapabilitiesStore.setState((state) => ({
      capabilities: {
        ...state.capabilities,
        platform: 'android',
        isMobile: true,
        windowControls: false,
        localShell: false,
        agentGateway: false,
        desktopUpdater: false,
        cliIpc: false,
        directoryTransfer: false,
        backgroundTunnels: false,
      },
      status: 'ready',
    }));
  });

  afterEach(cleanup);

  it('accepts pasted key content without exposing the desktop file picker', () => {
    render(
      <ConnectDialog
        isOpen
        server={keyServer}
        onClose={() => {}}
        onConnected={() => {}}
      />
    );

    expect(screen.getByRole('textbox', { name: 'SSH Private Key' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Browse' })).not.toBeInTheDocument();
  });
});
