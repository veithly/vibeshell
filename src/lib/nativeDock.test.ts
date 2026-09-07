// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: any }) => unknown>(),
  emit: vi.fn(), invoke: vi.fn(), native: {
    label: 'detach-test', innerPosition: vi.fn(), scaleFactor: vi.fn(), outerPosition: vi.fn(),
    setPosition: vi.fn(), setFocus: vi.fn(), startDragging: vi.fn(),
  },
}));
vi.mock('@tauri-apps/api/event', () => ({
  emitTo: mocks.emit,
  listen: vi.fn(async (name, callback) => { mocks.handlers.set(name, callback); return () => { mocks.handlers.delete(name); }; }),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => mocks.native }));
vi.mock('./tauri', () => ({ safeInvoke: mocks.invoke }));
import { dragNativeWindow, receiveWindowDrops, requestDock } from './nativeDock';
import { useDetachedOwnership, type TransferSnapshot } from './detach';
const snapshot: TransferSnapshot = {
  target: { kind: 'terminal', sessionId: 'source', title: 'Shell' },
  session: { id: 'source', serverId: 'zsh', serverName: 'Shell', sessionType: 'local', state: 'connected', createdAt: 1788739200000, purpose: 'shell' },
};
const dropRequest = () => ({ id: 'transfer-1', source: 'detach-test', snapshot, point: { x: 110, y: 600 }, expiresAt: Date.now() + 7000 });

describe('native window docking handshake', () => {
  beforeEach(() => {
    vi.useFakeTimers(); vi.clearAllMocks(); mocks.handlers.clear();
    Object.defineProperty(window, '__TAURI_INTERNALS__', { configurable: true, value: {} });
    useDetachedOwnership.setState({ owners: { 'session:source': 'detach-test' } });
    mocks.emit.mockResolvedValue(undefined);
    mocks.native.innerPosition.mockResolvedValue({ x: 100, y: 200 });
    mocks.native.outerPosition.mockResolvedValue({ x: 100, y: 200 });
    mocks.native.scaleFactor.mockResolvedValue(2);
    mocks.native.setPosition.mockResolvedValue(undefined);
    mocks.native.setFocus.mockResolvedValue(undefined);
    mocks.native.startDragging.mockResolvedValue(undefined);
    const pane = document.createElement('div'); pane.dataset.paneId = 'session:target';
    pane.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 800, bottom: 400, width: 800, height: 400, toJSON: () => ({}) });
    document.body.replaceChildren(pane);
    Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => pane });
  });
  afterEach(() => { vi.useRealTimers(); document.body.replaceChildren(); delete (window as any).__TAURI_INTERNALS__; });
  it('converts physical coordinates to the correct pane edge and accepts a repeated request only once', async () => {
    const accept = vi.fn(() => true); const stop = await receiveWindowDrops(accept);
    const request = dropRequest();
    await mocks.handlers.get('vibeshell://dock-drop')!({ payload: request });
    await mocks.handlers.get('vibeshell://dock-drop')!({ payload: request });
    expect(accept).toHaveBeenCalledTimes(1);
    expect(accept).toHaveBeenCalledWith(snapshot, { paneId: 'session:target', side: 'left' });
    expect(mocks.emit).toHaveBeenLastCalledWith('detach-test', 'vibeshell://dock-ack', { id: request.id, accepted: true });
    stop();
  });
  it.each(['wrong-owner', 'expired', 'outside'])('rejects %s without moving content out of the source', async (reason) => {
    const accept = vi.fn(() => true); const stop = await receiveWindowDrops(accept);
    const request = dropRequest();
    if (reason === 'wrong-owner') request.source = 'detach-unknown';
    if (reason === 'expired') request.expiresAt = Date.now() - 1;
    if (reason === 'outside') request.point = { x: -100, y: -100 };
    await mocks.handlers.get('vibeshell://dock-drop')!({ payload: request });
    expect(accept).not.toHaveBeenCalled();
    expect(mocks.emit).toHaveBeenLastCalledWith(request.source, 'vibeshell://dock-ack', { id: request.id, accepted: false });
    expect(useDetachedOwnership.getState().owners['session:source']).toBe('detach-test'); stop();
  });
  it('waits for the matching acceptance rather than considering send success a transfer', async () => {
    const result = requestDock(snapshot);
    await vi.advanceTimersByTimeAsync(0);
    const request = mocks.emit.mock.calls[0][2];
    await mocks.handlers.get('vibeshell://dock-ack')!({ payload: { id: request.id, accepted: true } });
    await expect(result).resolves.toBe(true);
    expect(mocks.handlers.has('vibeshell://dock-ack')).toBe(false);
  });
  it('times out an unacknowledged handoff without releasing source ownership', async () => {
    const result = requestDock(snapshot);
    const check = expect(result).rejects.toThrow('did not confirm');
    await vi.advanceTimersByTimeAsync(8500); await check;
    expect(mocks.emit).toHaveBeenCalledTimes(4);
    expect(useDetachedOwnership.getState().owners['session:source']).toBe('detach-test');
  });
  it('does not dock while the mouse button remains down, and docks at the release point', async () => {
    mocks.invoke.mockResolvedValueOnce({ success: true, data: { x: 500, y: 240, primaryDown: true, escapeDown: false } })
      .mockResolvedValueOnce({ success: true, data: { x: 700, y: 400, primaryDown: true, escapeDown: false } })
      .mockResolvedValue({ success: true, data: { x: 710, y: 410, primaryDown: false, escapeDown: false } });
    const drop = vi.fn(async () => {}); const running = dragNativeWindow(drop);
    await vi.advanceTimersByTimeAsync(20); expect(drop).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(40); await running;
    expect(drop).toHaveBeenCalledTimes(1);
    expect(drop).toHaveBeenCalledWith(expect.objectContaining({ x: 710, y: 410 }));
    expect(document.body).not.toHaveClass('detached-window-dragging');
  });
  it('rounds fractional coordinates on the very first tear-out position', async () => {
    mocks.native.scaleFactor.mockResolvedValue(1.25);
    mocks.invoke.mockResolvedValue({ success: true, data: { x: 1390.140625, y: 310.125, primaryDown: false, escapeDown: false } });
    await dragNativeWindow(vi.fn(async () => {}), true);
    expect(mocks.native.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 1278, y: 288 }));
    for (const [position] of mocks.native.setPosition.mock.calls) {
      expect(Number.isInteger(position.x) && Number.isInteger(position.y)).toBe(true);
    }
  });
  it('cancels with Escape and restores the original native position', async () => {
    mocks.invoke.mockResolvedValue({ success: true, data: { x: 500, y: 240, primaryDown: true, escapeDown: true } });
    const drop = vi.fn(async () => {}); await dragNativeWindow(drop);
    expect(drop).not.toHaveBeenCalled(); expect(mocks.native.setPosition).toHaveBeenCalledWith(expect.objectContaining({ x: 100, y: 200 }));
  });
});
