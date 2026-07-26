import { StrictMode } from 'react';
import { act, cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Terminal } from './Terminal';

const mocks = vi.hoisted(() => {
  const operations: string[] = [];
  const construct = vi.fn();
  const open = vi.fn(() => operations.push('open'));
  const dispose = vi.fn(() => operations.push('dispose'));
  const unlisten = vi.fn();
  const listen = vi.fn(async () => {
    operations.push('listen');
    return unlisten;
  });
  const attachLocalShellSession = vi.fn(async () => {
    operations.push('attach');
    return true;
  });
  const noOpAsync = vi.fn(async () => true);
  const noOp = vi.fn();

  const completionState = {
    items: [],
    selectedIndex: 0,
    position: { x: 0, y: 0 },
    visible: false,
    currentInput: '',
    ghostText: '',
  };
  const completionActions = {
    hideCompletions: vi.fn(),
    clearGhostText: vi.fn(),
    showCompletions: vi.fn(),
    getCompletionText: vi.fn(() => ''),
    getSelectedItem: vi.fn(() => null),
    selectPrev: vi.fn(),
    selectNext: vi.fn(),
    addToHistory: vi.fn(),
    updateCompletions: vi.fn(),
    autoTrigger: vi.fn(),
    setSelectedIndex: vi.fn(),
  };

  const storeState = {
    sessions: [{
      id: 'agent-session',
      serverId: 'agent:codex',
      serverName: 'Codex',
      state: 'connected',
      createdAt: 0,
      sessionType: 'local',
      purpose: 'coding_agent',
    }],
    sendInput: noOpAsync,
    sendInputFast: noOp,
    resizeSession: noOpAsync,
    attachSession: noOpAsync,
    attachLocalShellSession,
    detachLocalShellSession: noOpAsync,
    detachSession: noOpAsync,
    sendLocalShellInput: noOpAsync,
    sendLocalShellInputFast: noOp,
    resizeLocalShellSession: noOpAsync,
  };

  return {
    operations,
    construct,
    open,
    dispose,
    listen,
    unlisten,
    attachLocalShellSession,
    noOpAsync,
    noOp,
    completionState,
    completionActions,
    storeState,
    flushInputBatch: vi.fn(),
  };
});

vi.mock('@xterm/xterm', () => ({
  Terminal: class TerminalMock {
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    element: HTMLDivElement;
    buffer = {
      active: {
        cursorX: 0,
        cursorY: 0,
        type: 'normal',
        viewportY: 0,
        baseY: 0,
      },
    };
    modes = { mouseTrackingMode: 'none' };

    constructor(options: Record<string, unknown>) {
      mocks.operations.push('construct');
      mocks.construct(options);
      this.options = { ...options };
      this.element = document.createElement('div');
      const screen = document.createElement('div');
      screen.className = 'xterm-screen';
      this.element.append(screen);
    }

    loadAddon() {}
    open() { mocks.open(); }
    attachCustomKeyEventHandler() {}
    onData() { return { dispose: vi.fn() }; }
    onResize() { return { dispose: vi.fn() }; }
    write() {}
    writeln() {}
    paste() {}
    focus() {}
    clear() {}
    selectAll() {}
    getSelection() { return ''; }
    hasSelection() { return false; }
    dispose() { mocks.dispose(); }
  },
}));

vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class FitAddonMock {
    fit() { mocks.operations.push('fit'); }
  },
}));

vi.mock('@xterm/addon-web-links', () => ({
  WebLinksAddon: class WebLinksAddonMock {},
}));

vi.mock('@xterm/addon-webgl', () => ({
  WebglAddon: class WebglAddonMock {
    onContextLoss() {}
    dispose() {}
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: mocks.listen,
}));

vi.mock('../../stores/sessionStore', () => ({
  useSessionStore: (selector: (state: typeof mocks.storeState) => unknown) => selector(mocks.storeState),
}));

vi.mock('../../stores/settingsStore', () => ({
  useSettingsStore: () => ({
    settings: {
      terminal: {
        fontSize: 14,
        fontFamily: 'JetBrains Mono',
        cursorBlink: true,
        cursorStyle: 'block',
        scrollbackLines: 10_000,
      },
      appearance: { theme: 'paper-white' },
      aiPrediction: { enabled: false },
    },
  }),
  themes: [{
    name: 'paper-white',
    colors: {
      bg: '#ffffff',
      bgDark: '#f7f7f5',
      bgHl: '#e7e7e3',
      fg: '#171717',
      fgDark: '#73736e',
      accent: '#6d4aff',
      onAccent: '#ffffff',
      red: '#b52f3e',
      green: '#18714a',
      yellow: '#805200',
      magenta: '#743796',
      cyan: '#087477',
      orange: '#974117',
    },
  }],
}));

vi.mock('./useCompletion', () => ({
  useCompletion: () => [mocks.completionState, mocks.completionActions],
}));

vi.mock('./CompletionPopup', () => ({
  CompletionPopup: () => null,
}));

vi.mock('./MobileKeyBar', () => ({
  MobileKeyBar: () => null,
}));

vi.mock('../../lib/tauri', () => ({
  flushInputBatch: mocks.flushInputBatch,
}));

class ResizeObserverMock {
  observe() {}
  disconnect() {}
}

describe('Terminal lifecycle', () => {
  let nextFrameId = 0;
  let frames: Map<number, FrameRequestCallback>;

  const flushAnimationFrames = () => {
    const pending = [...frames.values()];
    frames.clear();
    pending.forEach((callback) => callback(0));
  };

  beforeEach(() => {
    mocks.operations.length = 0;
    mocks.construct.mockClear();
    mocks.open.mockClear();
    mocks.dispose.mockClear();
    mocks.listen.mockClear();
    mocks.unlisten.mockClear();
    mocks.attachLocalShellSession.mockClear();
    mocks.flushInputBatch.mockClear();
    mocks.listen.mockImplementation(async () => {
      mocks.operations.push('listen');
      return mocks.unlisten;
    });
    mocks.attachLocalShellSession.mockImplementation(async () => {
      mocks.operations.push('attach');
      return true;
    });

    nextFrameId = 0;
    frames = new Map();
    vi.stubGlobal('ResizeObserver', ResizeObserverMock);
    vi.stubGlobal('requestAnimationFrame', vi.fn((callback: FrameRequestCallback) => {
      const id = ++nextFrameId;
      frames.set(id, callback);
      return id;
    }));
    vi.stubGlobal('cancelAnimationFrame', vi.fn((id: number) => {
      frames.delete(id);
    }));
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('constructs once under StrictMode and attaches only after the terminal opens', async () => {
    render(
      <StrictMode>
        <Terminal sessionId="agent-session" />
      </StrictMode>
    );

    expect(mocks.construct).not.toHaveBeenCalled();
    expect(mocks.attachLocalShellSession).not.toHaveBeenCalled();
    expect(frames.size).toBe(1);

    await act(async () => {
      flushAnimationFrames();
      await Promise.resolve();
    });

    await waitFor(() => expect(mocks.attachLocalShellSession).toHaveBeenCalledOnce());
    expect(mocks.construct).toHaveBeenCalledOnce();
    expect(mocks.open).toHaveBeenCalledOnce();
    expect(mocks.operations.indexOf('open')).toBeLessThan(mocks.operations.indexOf('attach'));
  });

  it('cancels initialization when unmounted before the animation frame', () => {
    const { unmount } = render(
      <StrictMode>
        <Terminal sessionId="agent-session" />
      </StrictMode>
    );

    unmount();

    expect(() => flushAnimationFrames()).not.toThrow();
    expect(mocks.construct).not.toHaveBeenCalled();
    expect(mocks.attachLocalShellSession).not.toHaveBeenCalled();
  });

  it('does not fit xterm while its layout container has no renderable size', async () => {
    const { container } = render(<Terminal sessionId="agent-session" />);
    const terminal = container.querySelector('.terminal-viewport') as HTMLElement;
    vi.spyOn(terminal, 'getBoundingClientRect').mockReturnValue({
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      top: 0,
      right: 0,
      bottom: 0,
      left: 0,
      toJSON: () => ({}),
    });

    await act(async () => {
      flushAnimationFrames();
      await Promise.resolve();
    });
    window.dispatchEvent(new Event('resize'));

    expect(mocks.operations.filter((operation) => operation === 'fit')).toHaveLength(0);
  });
});
