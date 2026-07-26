import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SftpPanel } from '../SftpPanel';
import { useRuntimeCapabilitiesStore } from '../../../stores/runtimeCapabilitiesStore';

const safeInvokeMock = vi.fn();
const writeTextMock = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: (...args: unknown[]) => safeInvokeMock(...args),
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe('SftpPanel path copy', () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    useRuntimeCapabilitiesStore.setState({
      status: 'ready',
      capabilities: {
        platform: 'linux',
        isMobile: false,
        windowControls: true,
        localShell: true,
        agentGateway: true,
        desktopUpdater: true,
        cliIpc: true,
        directoryTransfer: true,
        backgroundTunnels: true,
      },
    });
    writeTextMock.mockReset();
    writeTextMock.mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextMock },
      configurable: true,
    });

    safeInvokeMock.mockReset();
    safeInvokeMock.mockImplementation((command: string, args?: { request?: { path?: string } }) => {
      if (command === 'sftp_init') {
        return Promise.resolve({ success: true, data: true });
      }
      if (command === 'sftp_pwd') {
        return Promise.resolve({ success: true, data: '/var/www' });
      }
      if (command === 'sftp_list_dir') {
        if (args?.request?.path === '/var/www/src') {
          return Promise.resolve({
            success: true,
            data: [
              {
                name: 'components',
                path: '/var/www/src/components',
                isDirectory: true,
                size: 0,
                modifiedAt: 1_700_000_000,
                permissions: 'drwxr-xr-x',
              },
              {
                name: 'index.ts',
                path: '/var/www/src/index.ts',
                isDirectory: false,
                size: 64,
                modifiedAt: 1_700_000_000,
                permissions: '-rw-r--r--',
              },
            ],
          });
        }
        if (args?.request?.path === '/var/www/src/components') {
          return Promise.resolve({
            success: true,
            data: [
              {
                name: 'Button.tsx',
                path: '/var/www/src/components/Button.tsx',
                isDirectory: false,
                size: 128,
                modifiedAt: 1_700_000_000,
                permissions: '-rw-r--r--',
              },
            ],
          });
        }
        return Promise.resolve({
          success: true,
          data: [
            {
              name: 'src',
              path: '/var/www/src',
              isDirectory: true,
              size: 0,
              modifiedAt: 1_700_000_000,
              permissions: 'drwxr-xr-x',
            },
            {
              name: 'app.log',
              path: '/var/www/app.log',
              isDirectory: false,
              size: 42,
              modifiedAt: 1_700_000_000,
              permissions: '-rw-r--r--',
            },
          ],
        });
      }
      return Promise.resolve({ success: true, data: null });
    });
  });

  it('copies a selected entry path from the context menu', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} />);

    const entry = await screen.findByText('app.log');
    fireEvent.contextMenu(entry);

    fireEvent.click(await screen.findByText('Copy Path'));

    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith('/var/www/app.log');
    });
  });

  it('copies the current directory path from the toolbar when nothing is selected', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} />);

    await screen.findByText('app.log');
    fireEvent.click(screen.getByTitle('Copy current path'));

    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith('/var/www');
    });
  });

  it('puts the address on its own row and collapses secondary actions in a narrow sidebar', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    await screen.findByText('app.log');
    const toolbar = screen.getByTestId('sftp-toolbar');
    const addressBar = screen.getByTestId('sftp-address-bar');
    expect(toolbar.contains(addressBar)).toBe(false);
    expect(screen.getByTitle('Home')).toBeInTheDocument();
    expect(screen.getByTitle('Upload file')).toBeInTheDocument();

    fireEvent.click(screen.getByTitle('More actions'));
    expect(await screen.findByText('Upload folder')).toBeInTheDocument();
    expect(screen.getByText('Sync current folder')).toBeInTheDocument();
  });

  it('hides path-based transfers when the runtime has no native file picker', async () => {
    useRuntimeCapabilitiesStore.setState((state) => ({
      capabilities: { ...state.capabilities, directoryTransfer: false },
    }));

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    await screen.findByText('app.log');
    expect(screen.queryByTitle('Upload file')).not.toBeInTheDocument();
    fireEvent.click(screen.getByTitle('More actions'));
    expect(screen.queryByText('Upload folder')).not.toBeInTheDocument();
    expect(screen.queryByText('Sync current folder')).not.toBeInTheDocument();
    expect(screen.queryByText('Download')).not.toBeInTheDocument();
  });

  it('opens directories with one tap on coarse-pointer devices', async () => {
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: query === '(pointer: coarse)',
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    await screen.findByText('app.log');
    fireEvent.click(screen.getByTitle('Details view'));
    fireEvent.click(screen.getByText('src'));

    expect(await screen.findByText('index.ts')).toBeInTheDocument();
  });

  it('docks on the right and expands nested paths as Finder-style columns', async () => {
    const { container } = render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />
    );

    await screen.findByText('app.log');
    expect(container.firstElementChild).toHaveStyle({ width: '420px', height: '100%' });

    fireEvent.click(screen.getByTitle('Column view'));
    expect(container.firstElementChild).toHaveStyle({ width: '680px', height: '100%' });

    fireEvent.click(screen.getByTitle('src'));
    await screen.findByText('components');
    expect(container.querySelectorAll('[data-sftp-column]')).toHaveLength(2);

    fireEvent.click(screen.getByTitle('components'));
    await screen.findByText('Button.tsx');
    expect(container.querySelectorAll('[data-sftp-column]')).toHaveLength(3);

    fireEvent.click(screen.getByTitle('Icon view'));
    expect(screen.getByTitle('Button.tsx')).toHaveClass('aspect-square');
  });

  it('keeps fullscreen interactive when the panel header is clicked', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    await screen.findByText('app.log');
    fireEvent.click(screen.getByLabelText('Enter fullscreen'));

    const panel = screen.getByTestId('sftp-panel');
    expect(panel).toHaveStyle({ width: '100%', height: '100%' });
    expect(panel).not.toHaveClass('pointer-events-none');

    fireEvent.click(screen.getByText('SFTP'));

    expect(screen.getByLabelText('Exit fullscreen')).toBeInTheDocument();
    expect(screen.getByTitle('Home')).toBeInTheDocument();
    expect(panel).not.toHaveClass('pointer-events-none');
  });
});
