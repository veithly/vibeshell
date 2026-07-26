import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';
import { useServerStore, type Server } from '../../stores/serverStore';
import { AddServerDialog } from './AddServerDialog';

const safeInvokeMock = vi.hoisted(() => vi.fn());

vi.mock('../../lib/tauri', () => ({
  safeInvoke: safeInvokeMock,
}));

afterEach(cleanup);

describe('AddServerDialog credential storage', () => {
  const createdServer: Server = {
    id: 'created-server',
    name: 'Production',
    host: 'prod.example.com',
    port: 22,
    username: 'root',
    auth_type: 'password',
    tags: [],
    created_at: 1,
    updated_at: 1,
  };
  const addServer = vi.fn().mockResolvedValue(createdServer);
  const updateServer = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    safeInvokeMock.mockReset().mockResolvedValue({ success: true, data: 'credential-1' });
    addServer.mockClear();
    updateServer.mockClear();
    useServerStore.setState({
      servers: [],
      groups: [],
      loading: false,
      error: null,
      addServer,
      updateServer,
    });
    useRuntimeCapabilitiesStore.setState((state) => ({
      capabilities: {
        ...state.capabilities,
        platform: 'macos',
        isMobile: false,
        windowControls: true,
        localShell: true,
        agentGateway: true,
        desktopUpdater: true,
        cliIpc: true,
        directoryTransfer: true,
        backgroundTunnels: true,
      },
      status: 'ready',
    }));
  });

  it('requires an explicit opt-in before saving credentials locally', () => {
    render(<AddServerDialog isOpen onClose={() => {}} />);

    expect(
      screen.getByRole('checkbox', { name: 'Save credentials on this device' })
    ).not.toBeChecked();
  });

  it('uses pasted key content instead of a desktop file picker on mobile', () => {
    useRuntimeCapabilitiesStore.setState((state) => ({
      capabilities: {
        ...state.capabilities,
        platform: 'ios',
        isMobile: true,
        windowControls: false,
        localShell: false,
        agentGateway: false,
        desktopUpdater: false,
        cliIpc: false,
        directoryTransfer: false,
        backgroundTunnels: false,
      },
    }));
    render(<AddServerDialog isOpen onClose={() => {}} />);

    fireEvent.click(screen.getByRole('button', { name: 'SSH Key' }));

    expect(screen.getByRole('textbox', { name: 'Private Key' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Browse' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('checkbox', { name: 'Save credentials on this device' })
    ).not.toBeInTheDocument();
  });

  it('creates the server before saving name-keyed credentials', async () => {
    render(<AddServerDialog isOpen onClose={() => {}} />);
    fireEvent.change(screen.getByPlaceholderText('My Server'), {
      target: { value: 'Production' },
    });
    fireEvent.change(screen.getByPlaceholderText('192.168.1.1 or example.com'), {
      target: { value: 'prod.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('Enter password...'), {
      target: { value: 'local-secret' },
    });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Save credentials on this device' }));
    fireEvent.click(screen.getByRole('button', { name: 'Add Server' }));

    await waitFor(() => expect(updateServer).toHaveBeenCalledWith('created-server', {
      credential_id: 'credential-1',
    }));
    expect(addServer.mock.invocationCallOrder[0]).toBeLessThan(
      safeInvokeMock.mock.invocationCallOrder[0]
    );
    expect(safeInvokeMock).toHaveBeenCalledWith('save_credential', {
      request: expect.objectContaining({
        serverName: 'Production',
        credential: 'local-secret',
      }),
    });
  });
});
