import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useRuntimeCapabilitiesStore } from '../stores/runtimeCapabilitiesStore';
import { TitleBar } from './TitleBar';

const windowApi = vi.hoisted(() => ({
  isFullscreen: vi.fn(),
  isMaximized: vi.fn(),
  setFullscreen: vi.fn(),
  toggleMaximize: vi.fn(),
  minimize: vi.fn(),
  close: vi.fn(),
}));
const getCurrentWindowMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: getCurrentWindowMock,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}));

const macosCapabilities = {
  platform: 'macos',
  isMobile: false,
  windowControls: true,
  localShell: true,
  agentGateway: true,
  desktopUpdater: true,
  cliIpc: true,
  directoryTransfer: true,
  backgroundTunnels: true,
} as const;

describe('TitleBar window controls', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
    Object.defineProperty(navigator, 'userAgent', { value: 'Mozilla/5.0 (Macintosh)', configurable: true });
    windowApi.isFullscreen.mockReset().mockResolvedValue(false);
    windowApi.isMaximized.mockReset().mockResolvedValue(false);
    windowApi.setFullscreen.mockReset().mockResolvedValue(undefined);
    windowApi.toggleMaximize.mockReset().mockResolvedValue(undefined);
    windowApi.minimize.mockReset().mockResolvedValue(undefined);
    windowApi.close.mockReset().mockResolvedValue(undefined);
    getCurrentWindowMock.mockReset().mockReturnValue(windowApi);
    listenMock.mockReset().mockResolvedValue(() => {});
    useRuntimeCapabilitiesStore.setState({
      capabilities: macosCapabilities,
      status: 'ready',
    });
  });

  afterEach(cleanup);

  it('uses native fullscreen for the macOS green button', async () => {
    render(<TitleBar />);

    fireEvent.click(await screen.findByLabelText('Enter fullscreen'));

    await waitFor(() => expect(windowApi.setFullscreen).toHaveBeenCalledWith(true));
    expect(windowApi.toggleMaximize).not.toHaveBeenCalled();
  });

  it('exits fullscreen before minimizing on macOS', async () => {
    windowApi.isFullscreen.mockResolvedValue(true);
    render(<TitleBar />);

    fireEvent.click(await screen.findByLabelText('Minimize'));

    await waitFor(() => expect(windowApi.minimize).toHaveBeenCalled(), { timeout: 1500 });
    expect(windowApi.setFullscreen).toHaveBeenCalledWith(false);
    expect(windowApi.setFullscreen.mock.invocationCallOrder[0])
      .toBeLessThan(windowApi.minimize.mock.invocationCallOrder[0]);
  });

  it('exits fullscreen before closing on macOS', async () => {
    windowApi.isFullscreen.mockResolvedValue(true);
    render(<TitleBar />);

    fireEvent.click(await screen.findByLabelText('Close'));

    await waitFor(() => expect(windowApi.close).toHaveBeenCalled(), { timeout: 1500 });
    expect(windowApi.setFullscreen).toHaveBeenCalledWith(false);
    expect(windowApi.setFullscreen.mock.invocationCallOrder[0])
      .toBeLessThan(windowApi.close.mock.invocationCallOrder[0]);
  });

  it('uses backend capabilities instead of a desktop user agent on mobile', async () => {
    useRuntimeCapabilitiesStore.setState({
      capabilities: {
        ...macosCapabilities,
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
    });

    render(<TitleBar />);
    await Promise.resolve();

    expect(screen.queryByLabelText('Close')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('Minimize')).not.toBeInTheDocument();
    expect(getCurrentWindowMock).not.toHaveBeenCalled();
    expect(listenMock).not.toHaveBeenCalled();
  });
});
