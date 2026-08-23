import { useEffect, useRef, useImperativeHandle, forwardRef, useCallback, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import '@xterm/xterm/css/xterm.css';
import { useSessionStore } from '../../stores/sessionStore';
import { useSettingsStore, themes } from '../../stores/settingsStore';
import { CompletionPopup, type CompletionItem } from './CompletionPopup';
import { MobileKeyBar } from './MobileKeyBar';
import { useCompletion } from './useCompletion';
import { applyTrackedInput, getClickedInputPosition, getCursorMoveSequence } from './inputCursor';
import { fireAndForgetInvoke, flushInputBatch } from '../../lib/tauri';

type ConnectionStatus = 'initializing' | 'listening' | 'receiving' | 'error';

interface AgentTerminalInputEvent {
  id: string;
  sessionId: string;
  text: string;
  kind: 'input' | 'typing' | 'exec';
  timestamp: number;
}

const AGENT_NOTICE_LIFETIME_MS = 12_000;

function fitTerminalIfRenderable(fitAddon: FitAddon, terminalElement: HTMLElement | null): boolean {
  if (!terminalElement?.isConnected) return false;

  const rect = terminalElement.getBoundingClientRect();
  if (
    !Number.isFinite(rect.width)
    || !Number.isFinite(rect.height)
    || rect.width <= 0
    || rect.height <= 0
  ) {
    return false;
  }

  fitAddon.fit();
  return true;
}

function decorateAgentTyping(terminal: XTerm, text: string, color: string) {
  if (terminal.buffer.active.type !== 'normal') return;

  const normalized = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
  let rowOffset = 0;
  let column = terminal.buffer.active.cursorX;
  let segmentStart = column;
  let segmentWidth = 0;

  const flushSegment = () => {
    if (segmentWidth === 0) return;
    const marker = terminal.registerMarker(rowOffset);
    if (marker) {
      terminal.registerDecoration({
        marker,
        x: segmentStart,
        width: segmentWidth,
        foregroundColor: color,
        layer: 'top',
      });
    }
    segmentWidth = 0;
  };

  for (const char of normalized) {
    if (char === '\n') {
      flushSegment();
      rowOffset += 1;
      column = 0;
      segmentStart = 0;
      continue;
    }

    segmentWidth += 1;
    column += 1;
    if (column >= terminal.cols) {
      flushSegment();
      rowOffset += 1;
      column = 0;
      segmentStart = 0;
    }
  }
  flushSegment();
}

function getTerminalScreenRect(xterm: XTerm, fallbackElement: HTMLElement): DOMRect {
  const screen = xterm.element?.querySelector('.xterm-screen') as HTMLElement | null;
  return (screen ?? fallbackElement).getBoundingClientRect();
}

function getCompletionPosition(
  xterm: XTerm,
  terminalElement: HTMLElement,
  options: { afterCursor?: boolean; viewportRelative?: boolean } = {}
): { x: number; y: number } {
  const screenRect = getTerminalScreenRect(xterm, terminalElement);
  const terminalRect = terminalElement.getBoundingClientRect();
  const cellWidth = screenRect.width / Math.max(1, xterm.cols);
  const cellHeight = screenRect.height / Math.max(1, xterm.rows);
  const cursorX = Math.min(xterm.cols - 1, xterm.buffer.active.cursorX + (options.afterCursor ? 1 : 0));
  const cursorY = Math.min(xterm.rows - 1, xterm.buffer.active.cursorY);
  const baseX = options.viewportRelative ? screenRect.left : screenRect.left - terminalRect.left;
  const baseY = options.viewportRelative ? screenRect.top : screenRect.top - terminalRect.top;

  return {
    x: baseX + (cursorX * cellWidth),
    y: baseY + ((cursorY + 1) * cellHeight) + 6,
  };
}

function getGhostTextPosition(
  xterm: XTerm,
  terminalElement: HTMLElement
): { x: number; y: number } {
  const screenRect = getTerminalScreenRect(xterm, terminalElement);
  const terminalRect = terminalElement.getBoundingClientRect();
  const cellWidth = screenRect.width / Math.max(1, xterm.cols);
  const cellHeight = screenRect.height / Math.max(1, xterm.rows);

  return {
    x: (screenRect.left - terminalRect.left) + (xterm.buffer.active.cursorX * cellWidth),
    y: (screenRect.top - terminalRect.top) + (xterm.buffer.active.cursorY * cellHeight),
  };
}

function throttle<T extends (...args: Parameters<T>) => void>(
  func: T,
  limit: number
): (...args: Parameters<T>) => void {
  let lastCall = 0;
  let timeoutId: ReturnType<typeof setTimeout> | null = null;

  return (...args: Parameters<T>) => {
    const now = Date.now();

    if (now - lastCall >= limit) {
      lastCall = now;
      func(...args);
    } else if (!timeoutId) {
      timeoutId = setTimeout(() => {
        lastCall = Date.now();
        timeoutId = null;
        func(...args);
      }, limit - (now - lastCall));
    }
  };
}

function useSessionType(sessionId: string | undefined): 'local' | 'ssh' | undefined {
  return useSessionStore((state) => {
    if (!sessionId) return undefined;
    const session = state.sessions.find((s) => s.id === sessionId);
    return session?.sessionType;
  });
}

function useSessionServerId(sessionId: string | undefined): string | undefined {
  return useSessionStore((state) => {
    if (!sessionId) return undefined;
    return state.sessions.find((session) => session.id === sessionId)?.serverId;
  });
}

function useIsCodingAgentSession(sessionId: string | undefined): boolean {
  return useSessionStore((state) => {
    if (!sessionId) return false;
    return state.sessions.find((session) => session.id === sessionId)?.purpose === 'coding_agent';
  });
}

interface TerminalProps {
  sessionId?: string;
  onData?: (data: string) => void;
}

export interface TerminalHandle {
  write: (data: string | Uint8Array) => void;
  writeln: (data: string) => void;
  clear: () => void;
  focus: () => void;
  getDimensions: () => { cols: number; rows: number } | null;
  sendCommand: (command: string) => void;
}

export const Terminal = forwardRef<TerminalHandle, TerminalProps>(
  function Terminal({ sessionId, onData }, ref) {
    const terminalRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<XTerm | null>(null);
    const fitAddonRef = useRef<FitAddon | null>(null);
    const eventUnlistenRef = useRef<(() => void) | null>(null);
    const [_connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('initializing');
    const [terminalInitializationFor, setTerminalInitializationFor] = useState<string | null | undefined>(null);
    const [terminalReadyFor, setTerminalReadyFor] = useState<string | null | undefined>(null);
    const receivedDataRef = useRef(false);
    const [agentNotices, setAgentNotices] = useState<AgentTerminalInputEvent[]>([]);
    const agentNoticeTimersRef = useRef<Map<string, number>>(new Map());
    const agentInputColorRef = useRef(themes[0].colors.magenta);

    const inputBufferRef = useRef<string>('');
    const cursorPositionRef = useRef<number>(0);

    const sendInput = useSessionStore((s) => s.sendInput);
    const sendInputFast = useSessionStore((s) => s.sendInputFast);
    const resizeSession = useSessionStore((s) => s.resizeSession);
    const attachSession = useSessionStore((s) => s.attachSession);
    const attachLocalShellSession = useSessionStore((s) => s.attachLocalShellSession);
    const detachLocalShellSession = useSessionStore((s) => s.detachLocalShellSession);
    const detachSession = useSessionStore((s) => s.detachSession);
    const sendLocalShellInput = useSessionStore((s) => s.sendLocalShellInput);
    const sendLocalShellInputFast = useSessionStore((s) => s.sendLocalShellInputFast);
    const resizeLocalShellSession = useSessionStore((s) => s.resizeLocalShellSession);
    const { settings } = useSettingsStore();

    const sessionType = useSessionType(sessionId);
    const serverId = useSessionServerId(sessionId);
    const isLocalShell = sessionType === 'local';
    const isCodingAgent = useIsCodingAgentSession(sessionId);
    const rawInputRef = useRef(isCodingAgent);

    const sendInputForSession = isLocalShell ? sendLocalShellInput : sendInput;
    const sendInputFastForSession = isLocalShell ? sendLocalShellInputFast : sendInputFast;
    const resizeSessionForSession = isLocalShell ? resizeLocalShellSession : resizeSession;

    const sendInputFastRef = useRef(sendInputFastForSession);
    const sendInputForSessionRef = useRef(sendInputForSession);
    const resizeForSessionRef = useRef(resizeSessionForSession);
    const onDataRef = useRef(onData);

    useEffect(() => { sendInputFastRef.current = sendInputFastForSession; }, [sendInputFastForSession]);
    useEffect(() => { sendInputForSessionRef.current = sendInputForSession; }, [sendInputForSession]);
    useEffect(() => { resizeForSessionRef.current = resizeSessionForSession; }, [resizeSessionForSession]);
    useEffect(() => { onDataRef.current = onData; }, [onData]);
    useEffect(() => {
      const currentTheme = themes.find((theme) => theme.name === settings.appearance.theme);
      agentInputColorRef.current = (currentTheme ?? themes[0]).colors.magenta;
    }, [settings.appearance.theme]);
    const [completionState, completionActions] = useCompletion(
      settings.aiPrediction,
      serverId ? `server:${serverId}` : 'global'
    );

    const [contextMenu, setContextMenu] = useState({
      visible: false,
      x: 0,
      y: 0,
    });

    const completionStateRef = useRef(completionState);
    const completionActionsRef = useRef(completionActions);

    useEffect(() => {
      completionStateRef.current = completionState;
    }, [completionState]);

    useEffect(() => {
      completionActionsRef.current = completionActions;
    }, [completionActions]);

    useEffect(() => {
      rawInputRef.current = isCodingAgent;
      if (isCodingAgent) {
        inputBufferRef.current = '';
        cursorPositionRef.current = 0;
        completionActionsRef.current.hideCompletions();
        completionActionsRef.current.clearGhostText();
      }
    }, [isCodingAgent]);

    const sessionIdRef = useRef(sessionId);
    const serverIdRef = useRef(serverId);
    useEffect(() => { sessionIdRef.current = sessionId; }, [sessionId]);
    useEffect(() => { serverIdRef.current = serverId; }, [serverId]);

    useImperativeHandle(ref, () => ({
      write: (data: string | Uint8Array) => {
        xtermRef.current?.write(data);
      },
      writeln: (data: string) => {
        xtermRef.current?.writeln(data);
      },
      clear: () => {
        xtermRef.current?.clear();
      },
      focus: () => {
        xtermRef.current?.focus();
      },
      getDimensions: () => {
        if (xtermRef.current) {
          return {
            cols: xtermRef.current.cols,
            rows: xtermRef.current.rows,
          };
        }
        return null;
      },
      sendCommand: (command: string) => {
        const sid = sessionIdRef.current;
        const normalized = command.trim();
        if (!sid || !normalized) return;
        sendInputForSessionRef.current(sid, `\x15${normalized}\r`);
        inputBufferRef.current = '';
        cursorPositionRef.current = 0;
        completionActionsRef.current.hideCompletions();
        completionActionsRef.current.clearGhostText();
        if (!isLocalShell && !isCodingAgent && serverIdRef.current) {
          fireAndForgetInvoke('history_record', {
            input: { serverId: serverIdRef.current, command: normalized },
          });
        }
      },
    }), []);

    const handleTerminalResize = useRef(
      throttle((cols: number, rows: number) => {
        const sid = sessionIdRef.current;
        if (sid) {
          resizeForSessionRef.current(sid, cols, rows);
        }
      }, 100)
    ).current;

    const handleCopy = useCallback(async () => {
      const xterm = xtermRef.current;
      if (!xterm) return;

      const selection = xterm.getSelection();
      if (selection) {
        try {
          await navigator.clipboard.writeText(selection);
        } catch (err) {
          console.error('[Terminal] Failed to copy:', err);
        }
      }
    }, []);

    const handlePaste = useCallback(async () => {
      const sid = sessionIdRef.current;
      if (!sid) return;

      try {
        const text = await navigator.clipboard.readText();
        if (text) {
          const xterm = xtermRef.current;
          if (xterm) {
            // xterm adds bracketed-paste markers when the active TUI requests them.
            xterm.paste(text);
          } else {
            sendInputForSessionRef.current(sid, text);
          }
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          completionActionsRef.current.hideCompletions();
          completionActionsRef.current.clearGhostText();
        }
      } catch (err) {
        console.error('[Terminal] Failed to paste:', err);
      }
    }, []);

    const handleMobileKey = useCallback((data: string) => {
      const sid = sessionIdRef.current;
      if (!sid) return;

      completionActionsRef.current.hideCompletions();
      completionActionsRef.current.clearGhostText();
      sendInputFastRef.current(sid, data);
      onDataRef.current?.(data);
      xtermRef.current?.focus();
    }, []);

    const handleContextMenu = useCallback((e: React.MouseEvent) => {
      e.preventDefault();
      setContextMenu({
        visible: true,
        x: e.clientX,
        y: e.clientY,
      });
    }, []);

    const closeContextMenu = useCallback(() => {
      setContextMenu((prev) => ({ ...prev, visible: false }));
    }, []);

    useEffect(() => {
      if (!contextMenu.visible) return;

      const handleClick = () => closeContextMenu();
      window.addEventListener('click', handleClick);
      return () => window.removeEventListener('click', handleClick);
    }, [contextMenu.visible, closeContextMenu]);

    const isNativeTextInputTarget = (target: EventTarget | null): boolean => {
      if (!(target instanceof HTMLElement)) return false;
      return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable;
    };

    useEffect(() => {
      const handleKeyDown = (e: KeyboardEvent) => {
        // Preserve browser-native editing shortcuts inside text inputs/textarea/contenteditable
        if (isNativeTextInputTarget(e.target)) return;

        if ((e.ctrlKey || e.metaKey) && e.key === 'c') {
          const xterm = xtermRef.current;
          if (xterm && xterm.hasSelection()) {
            e.preventDefault();
            handleCopy();
          }
        }

        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'C') {
          e.preventDefault();
          handleCopy();
        }

        if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || (e.shiftKey && e.key === 'V'))) {
          e.preventDefault();
          handlePaste();
        }
      };

      window.addEventListener('keydown', handleKeyDown);
      return () => window.removeEventListener('keydown', handleKeyDown);
    }, [handleCopy, handlePaste]);

    const handleCompletionSelect = useCallback((item: CompletionItem) => {
      const actions = completionActionsRef.current;
      const sid = sessionIdRef.current;

      if (!xtermRef.current || !sid) {
        actions.hideCompletions();
        return;
      }

      const completionText = actions.getCompletionText();
      if (!completionText) {
        actions.hideCompletions();
        return;
      }

      const sendFn = sendInputForSessionRef.current;
      const currentInput = inputBufferRef.current;
      const parts = currentInput.trim().split(/\s+/);

      let textToInsert: string;

      if (item.isHistory) {
        const backspaces = '\x7f'.repeat(currentInput.length);
        sendFn(sid, backspaces);
        textToInsert = item.text;
      } else if (parts.length >= 2) {
        const lastPart = parts[parts.length - 1];
        const backspaces = '\x7f'.repeat(lastPart.length);
        sendFn(sid, backspaces);
        textToInsert = completionText;
      } else {
        const backspaces = '\x7f'.repeat(currentInput.length);
        sendFn(sid, backspaces);
        textToInsert = completionText;
      }

      sendFn(sid, textToInsert);

      if (item.isHistory) {
        inputBufferRef.current = item.text;
      } else if (parts.length >= 2) {
        parts[parts.length - 1] = completionText;
        inputBufferRef.current = parts.join(' ');
      } else {
        inputBufferRef.current = completionText;
      }
      cursorPositionRef.current = inputBufferRef.current.length;

      actions.hideCompletions();
      xtermRef.current?.focus();
    }, []);

    const handleCompletionSelectRef = useRef(handleCompletionSelect);
    useEffect(() => {
      handleCompletionSelectRef.current = handleCompletionSelect;
    }, [handleCompletionSelect]);

    useEffect(() => {
      if (!sessionId || terminalReadyFor !== sessionId) {
        return;
      }

      setConnectionStatus('initializing');
      receivedDataRef.current = false;

      let outputUnlisten: (() => void) | null = null;
      let agentInputUnlisten: (() => void) | null = null;
      let shouldDetach = false;
      let disposed = false;

      interface SessionOutputEvent {
        session_id?: string;
        sessionId?: string;
        data: number[];
      }

      const setupListener = async () => {
        try {
          const { listen } = await import('@tauri-apps/api/event');

          outputUnlisten = await listen<SessionOutputEvent>('session-output', (event) => {
            const payloadSessionId = event.payload?.session_id ?? event.payload?.sessionId;
            if (payloadSessionId === sessionId && event.payload?.data) {
              if (!receivedDataRef.current) {
                receivedDataRef.current = true;
                setConnectionStatus('receiving');
              }

              if (xtermRef.current) {
                xtermRef.current.write(new Uint8Array(event.payload.data));
              }
            }
          });

          agentInputUnlisten = await listen<AgentTerminalInputEvent>(
            'agent-terminal-input',
            (event) => {
              const input = event.payload;
              if (input?.sessionId !== sessionId || !input.text) return;

              if (input.kind !== 'typing') {
                setAgentNotices((current) => [...current, input].slice(-3));
                const existingTimer = agentNoticeTimersRef.current.get(input.id);
                if (existingTimer !== undefined) {
                  window.clearTimeout(existingTimer);
                }
                const timer = window.setTimeout(() => {
                  setAgentNotices((current) => current.filter((item) => item.id !== input.id));
                  agentNoticeTimersRef.current.delete(input.id);
                }, AGENT_NOTICE_LIFETIME_MS);
                agentNoticeTimersRef.current.set(input.id, timer);
              }

              // Shared-shell input is echoed by the remote PTY. Decorating the
              // cells changes only their presentation, so terminal cursor state
              // stays synchronized with the server and the command is not shown
              // twice.
              const xterm = xtermRef.current;
              if (input.kind === 'typing' && xterm) {
                decorateAgentTyping(xterm, input.text, agentInputColorRef.current);
              }
            }
          );

          if (disposed) {
            outputUnlisten();
            agentInputUnlisten();
            outputUnlisten = null;
            agentInputUnlisten = null;
            return;
          }

          eventUnlistenRef.current = outputUnlisten;

          let attached: boolean;
          if (!isLocalShell) {
            attached = await attachSession(sessionId);
          } else {
            // Local shell: attach replays the buffered output (including the
            // initial prompt) via the session-output event. No need to send a
            // carriage return — that would trigger a second, duplicate prompt.
            attached = await attachLocalShellSession(sessionId);
          }

          if (disposed) {
            if (attached) {
              const detach = isLocalShell ? detachLocalShellSession : detachSession;
              await detach(sessionId);
            }
            return;
          }

          shouldDetach = attached;

          setConnectionStatus('listening');
        } catch (error) {
          if (disposed) return;
          console.error('[Terminal] Failed to set up session output listener:', error);
          setConnectionStatus('error');
          if (xtermRef.current) {
            xtermRef.current.writeln(`\x1b[1;31m[Error]\x1b[0m Failed to set up listener: ${error}`);
          }
        }
      };

      setupListener();

      return () => {
        disposed = true;
        outputUnlisten?.();
        agentInputUnlisten?.();
        if (eventUnlistenRef.current === outputUnlisten) {
          eventUnlistenRef.current = null;
        }
        if (shouldDetach) {
          const detach = isLocalShell ? detachLocalShellSession : detachSession;
          detach(sessionId).catch((error) => {
            console.warn('[Terminal] Failed to detach session:', error);
          });
        }
      };
    }, [
      sessionId,
      terminalReadyFor,
      isLocalShell,
      attachSession,
      attachLocalShellSession,
      detachSession,
      detachLocalShellSession,
    ]);

    useEffect(() => () => {
      for (const timer of agentNoticeTimersRef.current.values()) {
        window.clearTimeout(timer);
      }
      agentNoticeTimersRef.current.clear();
    }, []);

    const getXtermTheme = useCallback(() => {
      const currentTheme = themes.find((t) => t.name === settings.appearance.theme);
      const baseColors = currentTheme?.colors || themes[0].colors;
      const isLight = currentTheme?.name === 'paper-white' || currentTheme?.name === 'warm-ivory';
      return {
        background: baseColors.bg,
        foreground: baseColors.fg,
        cursor: baseColors.accent,
        cursorAccent: baseColors.bg,
        selectionBackground: baseColors.bgHl,
        black: isLight ? baseColors.fg : baseColors.bgHl,
        red: baseColors.red,
        green: baseColors.green,
        yellow: baseColors.yellow,
        blue: baseColors.accent,
        magenta: baseColors.magenta,
        cyan: baseColors.cyan,
        white: isLight ? baseColors.fgDark : baseColors.fg,
        brightBlack: baseColors.fgDark,
        brightRed: baseColors.red,
        brightGreen: baseColors.green,
        brightYellow: baseColors.yellow,
        brightBlue: baseColors.accent,
        brightMagenta: baseColors.magenta,
        brightCyan: baseColors.cyan,
        brightWhite: baseColors.fg,
      };
    }, [settings.appearance.theme]);

    useEffect(() => {
      const initializationFrame = requestAnimationFrame(() => {
        setTerminalInitializationFor(sessionId);
      });

      return () => cancelAnimationFrame(initializationFrame);
    }, [sessionId]);

    useEffect(() => {
      if (terminalInitializationFor !== sessionId || !terminalRef.current) return;

      const terminalElement = terminalRef.current;

      const xterm = new XTerm({
        theme: getXtermTheme(),
        fontSize: settings.terminal.fontSize,
        fontFamily: `${settings.terminal.fontFamily}, Menlo, Monaco, Consolas, monospace`,
        cursorBlink: settings.terminal.cursorBlink,
        cursorStyle: settings.terminal.cursorStyle,
        scrollback: settings.terminal.scrollbackLines,
        fastScrollModifier: 'alt',
        fastScrollSensitivity: 5,
        smoothScrollDuration: 0,
        allowProposedApi: true,
        drawBoldTextInBrightColors: true,
        minimumContrastRatio: 4.5,
      });

      const fitAddon = new FitAddon();
      const webLinksAddon = new WebLinksAddon();

      xterm.loadAddon(fitAddon);
      xterm.loadAddon(webLinksAddon);

      try {
        const webglAddon = new WebglAddon();
        webglAddon.onContextLoss(() => {
          webglAddon.dispose();
        });
        xterm.loadAddon(webglAddon);
      } catch {
        console.info('[Terminal] WebGL not available, using canvas renderer');
      }

      xterm.open(terminalElement);
      xtermRef.current = xterm;
      fitAddonRef.current = fitAddon;
      fitTerminalIfRenderable(fitAddon, terminalElement);

      const terminalScreen = xterm.element?.querySelector('.xterm-screen') as HTMLElement | null;
      const handleTerminalClick = (event: MouseEvent) => {
        const sid = sessionIdRef.current;
        const target = event.target instanceof Element ? event.target : null;
        if (
          !sid ||
          rawInputRef.current ||
          event.button !== 0 ||
          event.altKey ||
          event.ctrlKey ||
          event.metaKey ||
          event.shiftKey ||
          xterm.buffer.active.type !== 'normal' ||
          xterm.buffer.active.viewportY !== xterm.buffer.active.baseY ||
          xterm.modes.mouseTrackingMode !== 'none' ||
          xterm.hasSelection() ||
          inputBufferRef.current.length === 0 ||
          target?.closest('a, .xterm-hover')
        ) {
          return;
        }

        const screen = terminalScreen ?? terminalRef.current;
        if (!screen) return;
        const rect = screen.getBoundingClientRect();
        if (
          event.clientX < rect.left ||
          event.clientX >= rect.right ||
          event.clientY < rect.top ||
          event.clientY >= rect.bottom
        ) {
          return;
        }

        const clickColumn = Math.min(
          xterm.cols - 1,
          Math.max(0, Math.floor((event.clientX - rect.left) / (rect.width / Math.max(1, xterm.cols))))
        );
        const clickRow = Math.min(
          xterm.rows - 1,
          Math.max(0, Math.floor((event.clientY - rect.top) / (rect.height / Math.max(1, xterm.rows))))
        );
        const currentPosition = cursorPositionRef.current;
        const targetPosition = getClickedInputPosition({
          clickColumn,
          clickRow,
          cursorColumn: xterm.buffer.active.cursorX,
          cursorRow: xterm.buffer.active.cursorY,
          terminalColumns: xterm.cols,
          inputLength: inputBufferRef.current.length,
          inputCursor: currentPosition,
        });
        const moveSequence = getCursorMoveSequence(currentPosition, targetPosition);
        if (!moveSequence) return;

        sendInputFastRef.current(sid, moveSequence);
        cursorPositionRef.current = targetPosition;
        completionActionsRef.current.hideCompletions();
        completionActionsRef.current.clearGhostText();
        xterm.focus();
      };
      terminalScreen?.addEventListener('click', handleTerminalClick);

      xterm.attachCustomKeyEventHandler((event: KeyboardEvent) => {
        if (rawInputRef.current) {
          return true;
        }
        if (event.ctrlKey && event.code === 'Space' && event.type === 'keydown') {
          event.preventDefault();
          const cs = completionStateRef.current;
          const ca = completionActionsRef.current;
          if (cs.visible) {
            ca.hideCompletions();
          } else {
            const input = inputBufferRef.current;
            if (input.trim() && terminalRef.current) {
              ca.showCompletions(input, getCompletionPosition(xterm, terminalRef.current, { viewportRelative: true }));
            }
          }
          return false;
        }
        return true;
      });

      xterm.onData((data) => {
        if (rawInputRef.current) {
          const sid = sessionIdRef.current;
          if (sid) {
            sendInputFastRef.current(sid, data);
          }
          onDataRef.current?.(data);
          return;
        }

        const compState = completionStateRef.current;
        const compActions = completionActionsRef.current;

        if (data === '\t') {
          if (compState.visible) {
            const selectedItem = compActions.getSelectedItem();
            if (selectedItem) {
              handleCompletionSelectRef.current(selectedItem);
              return;
            }
            compActions.hideCompletions();
          }
          compActions.clearGhostText();
        }

        if (data === '\x1b' && compState.visible) {
          compActions.hideCompletions();
          return;
        }

        if (compState.visible) {
          if (data === '\x1b[A') {
            compActions.selectPrev();
            return;
          }
          if (data === '\x1b[B') {
            compActions.selectNext();
            return;
          }
        }

        if (data === '\x1b[C' && compState.ghostText && inputBufferRef.current.length > 0) {
          const sid = sessionIdRef.current;
          if (sid) {
            sendInputFastRef.current(sid, compState.ghostText);
          }
          const cursor = cursorPositionRef.current;
          inputBufferRef.current =
            inputBufferRef.current.slice(0, cursor) +
            compState.ghostText +
            inputBufferRef.current.slice(cursor);
          cursorPositionRef.current = cursor + compState.ghostText.length;
          compActions.clearGhostText();
          if (compState.visible) {
            compActions.hideCompletions();
          }
          return;
        }

        const previousBuffer = inputBufferRef.current;
        const trackedInput = applyTrackedInput(
          previousBuffer,
          cursorPositionRef.current,
          data
        );
        inputBufferRef.current = trackedInput.buffer;
        cursorPositionRef.current = trackedInput.cursor;

        if (data === '\r' || data === '\n') {
          const command = previousBuffer.trim();
          if (command) {
            compActions.addToHistory(command);
            if (!isLocalShell && !isCodingAgent && serverIdRef.current) {
              fireAndForgetInvoke('history_record', {
                input: { serverId: serverIdRef.current, command },
              });
            }
          }
          compActions.hideCompletions();
          compActions.clearGhostText();
        } else if (data === '\x7f' || data === '\b') {
          if (cursorPositionRef.current !== inputBufferRef.current.length) {
            compActions.hideCompletions();
            compActions.clearGhostText();
          } else if (!inputBufferRef.current.trim()) {
            compActions.hideCompletions();
            compActions.clearGhostText();
          } else if (compState.visible) {
            compActions.updateCompletions(inputBufferRef.current);
          } else if (terminalRef.current) {
            compActions.autoTrigger(
              inputBufferRef.current,
              getCompletionPosition(xterm, terminalRef.current, { afterCursor: true, viewportRelative: true })
            );
          }
        } else if (!trackedInput.known) {
          compActions.hideCompletions();
          compActions.clearGhostText();
        } else if (data.length === 1 && !/[\x00-\x1F\x7F]/.test(data)) {
          if (cursorPositionRef.current !== inputBufferRef.current.length) {
            compActions.hideCompletions();
            compActions.clearGhostText();
          } else if (terminalRef.current) {
            const position = getCompletionPosition(xterm, terminalRef.current, {
              afterCursor: true,
              viewportRelative: true,
            });

            if (compState.visible) {
              compActions.updateCompletions(inputBufferRef.current);
            } else {
              compActions.autoTrigger(inputBufferRef.current, position);
            }
          }
        } else if (['\x03', '\x1b[3~', '\x15', '\x0b', '\x17'].includes(data)) {
          compActions.hideCompletions();
          compActions.clearGhostText();
        } else if ([
          '\x1b[D',
          '\x1b[C',
          '\x1b[H',
          '\x1bOH',
          '\x1b[F',
          '\x1bOF',
          '\x01',
          '\x05',
        ].includes(data)) {
          compActions.hideCompletions();
          compActions.clearGhostText();
        }

        const sid = sessionIdRef.current;
        if (sid) {
          sendInputFastRef.current(sid, data);
        }
        onDataRef.current?.(data);
      });

      xterm.onResize(({ cols, rows }) => {
        handleTerminalResize(cols, rows);
      });

      if (!sessionId) {
        xterm.writeln('\x1b[1;34mVibeShell Terminal\x1b[0m');
        xterm.writeln('\x1b[90mNo session - type to test local echo\x1b[0m');
        xterm.writeln('');
      }

      handleTerminalResize(xterm.cols, xterm.rows);

      let resizeObserver: ResizeObserver | null = null;
      let resizeAnimationFrame: number | null = null;
      let disposed = false;
      resizeObserver = new ResizeObserver(() => {
        if (resizeAnimationFrame !== null) {
          cancelAnimationFrame(resizeAnimationFrame);
        }
        resizeAnimationFrame = requestAnimationFrame(() => {
          resizeAnimationFrame = null;
          if (!disposed && fitAddonRef.current === fitAddon) {
            fitTerminalIfRenderable(fitAddon, terminalElement);
          }
        });
      });
      resizeObserver.observe(terminalElement);

      const handleWindowResize = () => {
        if (!disposed && fitAddonRef.current === fitAddon) {
          fitTerminalIfRenderable(fitAddon, terminalElement);
        }
      };
      window.addEventListener('resize', handleWindowResize);
      window.visualViewport?.addEventListener('resize', handleWindowResize);
      window.visualViewport?.addEventListener('scroll', handleWindowResize);

      setTerminalReadyFor(sessionId);

      return () => {
        disposed = true;
        terminalScreen?.removeEventListener('click', handleTerminalClick);
        resizeObserver?.disconnect();
        if (resizeAnimationFrame !== null) {
          cancelAnimationFrame(resizeAnimationFrame);
          resizeAnimationFrame = null;
        }
        window.removeEventListener('resize', handleWindowResize);
        window.visualViewport?.removeEventListener('resize', handleWindowResize);
        window.visualViewport?.removeEventListener('scroll', handleWindowResize);
        // Flush any buffered keystrokes for this session before tearing down
        // the terminal, so no dangling RAF fires an IPC into a stale session.
        flushInputBatch(sessionId);
        fitAddonRef.current = null;
        xtermRef.current = null;
        xterm.dispose();
      };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [sessionId, terminalInitializationFor]);

    useEffect(() => {
      const xterm = xtermRef.current;
      if (!xterm || terminalReadyFor !== sessionId) return;

      xterm.options.fontSize = settings.terminal.fontSize;
      xterm.options.fontFamily = `${settings.terminal.fontFamily}, Menlo, Monaco, Consolas, monospace`;
      xterm.options.cursorBlink = settings.terminal.cursorBlink;
      xterm.options.cursorStyle = settings.terminal.cursorStyle;
      xterm.options.scrollback = settings.terminal.scrollbackLines;
      xterm.options.theme = getXtermTheme();

      if (fitAddonRef.current) {
        fitTerminalIfRenderable(fitAddonRef.current, terminalRef.current);
      }
    }, [sessionId, terminalReadyFor, settings.terminal, settings.appearance.theme, getXtermTheme]);

    const currentTheme = themes.find((t) => t.name === settings.appearance.theme);
    const themeColors = currentTheme?.colors || themes[0].colors;
    const bgColor = themeColors.bg;

    return (
      <div className="terminal-mobile-shell relative">
        <div
          ref={terminalRef}
          className="terminal-viewport w-full min-h-0 flex-1 relative overflow-hidden"
          style={{ backgroundColor: bgColor }}
          onContextMenu={handleContextMenu}
        />

        {agentNotices.length > 0 && (
          <div
            className="pointer-events-none absolute right-3 top-3 z-20 flex max-w-[min(32rem,calc(100%-1.5rem))] flex-col items-end gap-1.5"
            aria-live="polite"
          >
            {agentNotices.map((notice) => (
              <div
                key={notice.id}
                className="flex max-w-full items-start gap-2 rounded-md border border-tokyo-magenta/40 bg-tokyo-bg-dark/95 px-2.5 py-1.5 shadow-lg"
              >
                <span className="mt-0.5 flex-shrink-0 rounded bg-tokyo-magenta/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-tokyo-magenta">
                  AI · {notice.kind === 'input' ? 'Shell' : 'Exec'}
                </span>
                <code className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-xs text-tokyo-magenta">
                  {notice.text}
                </code>
              </div>
            ))}
          </div>
        )}

        <MobileKeyBar
          onSend={handleMobileKey}
          onPaste={() => { void handlePaste().finally(() => xtermRef.current?.focus()); }}
        />

        <CompletionPopup
          items={completionState.items}
          selectedIndex={completionState.selectedIndex}
          position={completionState.position}
          visible={!isCodingAgent && completionState.visible}
          onSelect={handleCompletionSelect}
          onSelectionChange={completionActions.setSelectedIndex}
          onClose={completionActions.hideCompletions}
          currentInput={completionState.currentInput}
        />

        {contextMenu.visible && (
          <div
            className="fixed z-50 min-w-[160px] py-1 rounded-lg border"
            style={{
              left: contextMenu.x,
              top: contextMenu.y,
              backgroundColor: themeColors.bgDark,
              borderColor: themeColors.bgHl,
            }}
          >
            <button
              className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 transition-colors"
              style={{ color: themeColors.fg }}
              onClick={() => {
                handleCopy();
                closeContextMenu();
              }}
              onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = themeColors.bgHl)}
              onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'transparent')}
            >
              <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none">
                <rect x="5" y="5" width="8" height="8" rx="1" stroke="currentColor" strokeWidth="1.3" />
                <path d="M11 3H4a1 1 0 0 0-1 1v7" stroke="currentColor" strokeWidth="1.3" />
              </svg>
              Copy
              <span className="ml-auto text-xs" style={{ color: themeColors.fgDark }}>
                Ctrl+C
              </span>
            </button>
            <button
              className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 transition-colors"
              style={{ color: themeColors.fg }}
              onClick={() => {
                handlePaste();
                closeContextMenu();
              }}
              onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = themeColors.bgHl)}
              onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'transparent')}
            >
              <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none">
                <rect x="4" y="3" width="8" height="10" rx="1" stroke="currentColor" strokeWidth="1.3" />
                <path d="M6 3V2a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1" stroke="currentColor" strokeWidth="1.3" />
              </svg>
              Paste
              <span className="ml-auto text-xs" style={{ color: themeColors.fgDark }}>
                Ctrl+V
              </span>
            </button>
            <div className="my-1 border-t" style={{ borderColor: themeColors.bgHl }} />
            <button
              className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 transition-colors"
              style={{ color: themeColors.fg }}
              onClick={() => {
                xtermRef.current?.selectAll();
                closeContextMenu();
              }}
              onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = themeColors.bgHl)}
              onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'transparent')}
            >
              <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none">
                <rect x="2" y="2" width="12" height="12" rx="1" stroke="currentColor" strokeWidth="1.3" />
                <path d="M5 5h6M5 8h6M5 11h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
              </svg>
              Select All
              <span className="ml-auto text-xs" style={{ color: themeColors.fgDark }}>
                Ctrl+A
              </span>
            </button>
            <button
              className="w-full px-3 py-1.5 text-left text-sm flex items-center gap-2 transition-colors"
              style={{ color: themeColors.fg }}
              onClick={() => {
                xtermRef.current?.clear();
                closeContextMenu();
              }}
              onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = themeColors.bgHl)}
              onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = 'transparent')}
            >
              <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none">
                <path d="M2 4h12M5 4V3a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1" stroke="currentColor" strokeWidth="1.3" />
                <path d="M13 4v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4" stroke="currentColor" strokeWidth="1.3" />
              </svg>
              Clear
            </button>
          </div>
        )}

        {completionState.ghostText && (
          <GhostTextOverlay
            ghostText={completionState.ghostText}
            terminalRef={terminalRef}
            xtermRef={xtermRef}
            themeColors={themeColors}
          />
        )}
      </div>
    );
  }
);

function GhostTextOverlay({
  ghostText,
  terminalRef,
  xtermRef,
  themeColors,
}: {
  ghostText: string;
  terminalRef: React.RefObject<HTMLDivElement | null>;
  xtermRef: React.RefObject<XTerm | null>;
  themeColors: { fg: string; fgDark: string; bg: string };
}) {
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);
  const [fontSize, setFontSize] = useState(14);

  useEffect(() => {
    if (!xtermRef.current || !terminalRef.current || !ghostText) {
      setPosition(null);
      return;
    }

    const xterm = xtermRef.current;
    setPosition(getGhostTextPosition(xterm, terminalRef.current));

    setFontSize(xterm.options.fontSize || 14);
  }, [ghostText, terminalRef, xtermRef]);

  if (!position || !ghostText) return null;

  return (
    <div
      className="pointer-events-none font-mono whitespace-pre"
      style={{
        position: 'absolute',
        left: `${position.x}px`,
        top: `${position.y}px`,
        fontSize: `${fontSize}px`,
        color: themeColors.fgDark,
        opacity: 0.5,
        lineHeight: 1.2,
        zIndex: 10,
      }}
    >
      {ghostText}
      <span style={{ opacity: 0.4, fontSize: `${fontSize * 0.75}px`, marginLeft: '0.5em' }}>
        {'→'}
      </span>
    </div>
  );
}
