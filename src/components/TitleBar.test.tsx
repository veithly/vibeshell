import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { TitleBar } from './TitleBar';

const windowApi = vi.hoisted(() => ({
  isFullscreen: vi.fn(),
  isMaximized: vi.fn(),
  setFullscreen: vi.fn(),
  toggleMaximize: vi.fn(),
  minimize: vi.fn(),
  close: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => windowApi,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe('TitleBar window controls', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'platform', { value: 'MacIntel', configurable: true });
    Object.defineProperty(navigator, 'userAgent', { value: 'Mozilla/5.0 (Macintosh)', configurable: true });
    windowApi.isFullscreen.mockReset().mockResolvedValue(false);
    windowApi.isMaximized.mockReset().mockResolvedValue(false);
    windowApi.setFullscreen.mockReset().mockResolvedValue(undefined);
    windowApi.toggleMaximize.mockReset().mockResolvedValue(undefined);
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
});
