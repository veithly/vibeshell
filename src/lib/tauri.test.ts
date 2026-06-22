import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

describe('sendInputBatched', () => {
  const rafCallbacks: FrameRequestCallback[] = [];

  beforeEach(() => {
    vi.resetModules();
    invokeMock.mockReset().mockResolvedValue(undefined);
    rafCallbacks.length = 0;
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      value: {},
      configurable: true,
    });
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((callback) => {
      rafCallbacks.push(callback);
      return rafCallbacks.length;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
  });

  afterEach(() => {
    vi.restoreAllMocks();
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it('batches local shell input into one IPC call', async () => {
    const { sendInputBatched } = await import('./tauri');

    sendInputBatched('local-1', 'a', 'local_shell_send_input');
    sendInputBatched('local-1', 'b', 'local_shell_send_input');

    expect(invokeMock).not.toHaveBeenCalled();

    rafCallbacks[0](0);

    await vi.waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('local_shell_send_input', {
        request: {
          sessionId: 'local-1',
          data: 'ab',
        },
      });
    });
  });
});
