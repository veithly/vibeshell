import { useEffect, useRef, useImperativeHandle, forwardRef, useCallback, useState } from 'react';
import { Terminal as XTerm } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';
import '@xterm/xterm/css/xterm.css';
import { useSessionStore } from '../../stores/sessionStore';
import { useSettingsStore, themes } from '../../stores/settingsStore';
import { CompletionPopup, type CompletionItem } from './CompletionPopup';
import { useCompletion } from './useCompletion';

type ConnectionStatus = 'initializing' | 'listening' | 'receiving' | 'error';

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
}

export const Terminal = forwardRef<TerminalHandle, TerminalProps>(
  function Terminal({ sessionId, onData }, ref) {
    const terminalRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<XTerm | null>(null);
    const fitAddonRef = useRef<FitAddon | null>(null);
    const eventUnlistenRef = useRef<(() => void) | null>(null);
    const [_connectionStatus, setConnectionStatus] = useState<ConnectionStatus>('initializing');
    const receivedDataRef = useRef(false);

    const inputBufferRef = useRef<string>('');
    const cursorPositionRef = useRef<number>(0);

    const sendInput = useSessionStore((s) => s.sendInput);
    const sendInputFast = useSessionStore((s) => s.sendInputFast);
    const resizeSession = useSessionStore((s) => s.resizeSession);
    const attachSession = useSessionStore((s) => s.attachSession);
    const detachSession = useSessionStore((s) => s.detachSession);
    const sendLocalShellInput = useSessionStore((s) => s.sendLocalShellInput);
    const sendLocalShellInputFast = useSessionStore((s) => s.sendLocalShellInputFast);
    const resizeLocalShellSession = useSessionStore((s) => s.resizeLocalShellSession);
    const { settings } = useSettingsStore();

    const sessionType = useSessionType(sessionId);
    const isLocalShell = sessionType === 'local';

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

    const [completionState, completionActions] = useCompletion();

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
    }), []);

    const sessionIdRef = useRef(sessionId);
    useEffect(() => { sessionIdRef.current = sessionId; }, [sessionId]);

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
          const processedText = text.replace(/\r?\n/g, '\r');
          sendInputForSessionRef.current(sid, processedText);
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          completionActionsRef.current.hideCompletions();
          completionActionsRef.current.clearGhostText();
        }
      } catch (err) {
        console.error('[Terminal] Failed to paste:', err);
      }
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

      actions.hideCompletions();
      xtermRef.current?.focus();
    }, []);

    const handleCompletionSelectRef = useRef(handleCompletionSelect);
    useEffect(() => {
      handleCompletionSelectRef.current = handleCompletionSelect;
    }, [handleCompletionSelect]);

    useEffect(() => {
      if (!sessionId) {
        return;
      }

      setConnectionStatus('initializing');
      receivedDataRef.current = false;

      let unlisten: (() => void) | null = null;
      let shouldDetach = false;

      interface SessionOutputEvent {
        session_id?: string;
        sessionId?: string;
        data: number[];
      }

      const setupListener = async () => {
        try {
          const { listen } = await import('@tauri-apps/api/event');

          unlisten = await listen<SessionOutputEvent>('session-output', (event) => {
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

          eventUnlistenRef.current = unlisten;

          if (!isLocalShell) {
            shouldDetach = await attachSession(sessionId);
          }

          setConnectionStatus('listening');

          if (xtermRef.current && isLocalShell) {
            xtermRef.current.writeln('\x1b[1;32m[Ready]\x1b[0m Local shell initialized.');
          }
        } catch (error) {
          console.error('[Terminal] Failed to set up session output listener:', error);
          setConnectionStatus('error');
          if (xtermRef.current) {
            xtermRef.current.writeln(`\x1b[1;31m[Error]\x1b[0m Failed to set up listener: ${error}`);
          }
        }
      };

      setupListener();

      return () => {
        if (eventUnlistenRef.current) {
          eventUnlistenRef.current();
          eventUnlistenRef.current = null;
        }
        if (shouldDetach && !isLocalShell) {
          detachSession(sessionId).catch((error) => {
            console.warn('[Terminal] Failed to detach session:', error);
          });
        }
      };
    }, [sessionId, isLocalShell, attachSession, detachSession]);

    const getXtermTheme = useCallback(() => {
      const currentTheme = themes.find((t) => t.name === settings.appearance.theme);
      const baseColors = currentTheme?.colors || themes[0].colors;
      return {
        background: baseColors.bg,
        foreground: baseColors.fg,
        cursor: baseColors.accent,
        cursorAccent: baseColors.bg,
        selectionBackground: baseColors.bgHl,
        black: '#32344a',
        red: '#f7768e',
        green: '#9ece6a',
        yellow: '#e0af68',
        blue: '#7aa2f7',
        magenta: '#ad8ee6',
        cyan: '#449dab',
        white: '#787c99',
        brightBlack: '#444b6a',
        brightRed: '#ff7a93',
        brightGreen: '#b9f27c',
        brightYellow: '#ff9e64',
        brightBlue: '#7da6ff',
        brightMagenta: '#bb9af7',
        brightCyan: '#0db9d7',
        brightWhite: '#acb0d0',
      };
    }, [settings.appearance.theme]);

    useEffect(() => {
      if (!terminalRef.current) {
        return;
      }

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
        minimumContrastRatio: 1,
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

      xterm.open(terminalRef.current);
      fitAddon.fit();

      xtermRef.current = xterm;
      fitAddonRef.current = fitAddon;

      xterm.attachCustomKeyEventHandler((event: KeyboardEvent) => {
        if (event.ctrlKey && event.code === 'Space' && event.type === 'keydown') {
          event.preventDefault();
          const cs = completionStateRef.current;
          const ca = completionActionsRef.current;
          if (cs.visible) {
            ca.hideCompletions();
          } else {
            const input = inputBufferRef.current;
            if (input.trim() && terminalRef.current) {
              const termRect = terminalRef.current.getBoundingClientRect();
              const cellWidth = termRect.width / xterm.cols;
              const cellHeight = termRect.height / xterm.rows;
              const cursorX = xterm.buffer.active.cursorX;
              const cursorY = xterm.buffer.active.cursorY;
              const pos = {
                x: termRect.left + (cursorX * cellWidth),
                y: termRect.top + ((cursorY + 1) * cellHeight) + 5,
              };
              ca.showCompletions(input, pos);
            }
          }
          return false;
        }
        return true;
      });

      xterm.onData((data) => {
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
          inputBufferRef.current += compState.ghostText;
          cursorPositionRef.current = inputBufferRef.current.length;
          compActions.clearGhostText();
          if (compState.visible) {
            compActions.hideCompletions();
          }
          return;
        }

        const resetPredictedInputState = () => {
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          compActions.hideCompletions();
          compActions.clearGhostText();
        };

        const isAnsiCursorOrEditSequence = /^(?:\x1b\[[0-9;?]*[~A-Za-z]|\x1bO[0-9A-Za-z])$/.test(data);
        if (isAnsiCursorOrEditSequence) {
          resetPredictedInputState();
        }

        const shouldResetForControlChar = [
          '\t',
          '\x01',
          '\x02',
          '\x04',
          '\x05',
          '\x06',
          '\x0b',
          '\x0c',
          '\x0e',
          '\x10',
          '\x17',
        ].includes(data);

        if (shouldResetForControlChar) {
          resetPredictedInputState();
        }

        if (data === '\r' || data === '\n') {
          const command = inputBufferRef.current.trim();
          if (command) {
            compActions.addToHistory(command);
          }
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          compActions.hideCompletions();
          compActions.clearGhostText();
        } else if (data === '\x7f' || data === '\b') {
          if (inputBufferRef.current.length > 0) {
            inputBufferRef.current = inputBufferRef.current.slice(0, -1);
            cursorPositionRef.current = Math.max(0, cursorPositionRef.current - 1);
          }
          if (!inputBufferRef.current.trim()) {
            compActions.hideCompletions();
            compActions.clearGhostText();
          } else if (compState.visible) {
            compActions.updateCompletions(inputBufferRef.current);
          } else {
            compActions.autoTrigger(inputBufferRef.current, { x: 0, y: 0 });
          }
        } else if (data === '\x03' || data === '\x15') {
          resetPredictedInputState();
        } else if (data.length === 1 && !/[\x00-\x1F\x7F]/.test(data)) {
          inputBufferRef.current += data;
          cursorPositionRef.current += 1;

          if (terminalRef.current) {
            const termRect = terminalRef.current.getBoundingClientRect();
            const cellWidth = termRect.width / xterm.cols;
            const cellHeight = termRect.height / xterm.rows;
            const cursorX = xterm.buffer.active.cursorX;
            const cursorY = xterm.buffer.active.cursorY;

            const position = {
              x: termRect.left + ((cursorX + 1) * cellWidth),
              y: termRect.top + ((cursorY + 1) * cellHeight) + 5,
            };

            if (compState.visible) {
              compActions.updateCompletions(inputBufferRef.current);
            } else {
              compActions.autoTrigger(inputBufferRef.current, position);
            }
          }
        } else if (data.length > 1 && !isAnsiCursorOrEditSequence) {
          resetPredictedInputState();
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

      if (sessionId) {
        if (isLocalShell) {
          xterm.writeln('\x1b[1;34mVibeShell Terminal\x1b[0m');
          xterm.writeln(`\x1b[90mSession: ${sessionId}\x1b[0m`);
          xterm.writeln('\x1b[1;33m[Starting...]\x1b[0m Initializing local shell...');
          xterm.writeln('');
        }
      } else {
        xterm.writeln('\x1b[1;34mVibeShell Terminal\x1b[0m');
        xterm.writeln('\x1b[90mNo session - type to test local echo\x1b[0m');
        xterm.writeln('');
      }

      handleTerminalResize(xterm.cols, xterm.rows);

      let resizeObserver: ResizeObserver | null = null;
      if (terminalRef.current) {
        resizeObserver = new ResizeObserver(() => {
          requestAnimationFrame(() => {
            if (fitAddonRef.current) {
              fitAddonRef.current.fit();
            }
          });
        });
        resizeObserver.observe(terminalRef.current);
      }

      const handleWindowResize = () => {
        fitAddon.fit();
      };
      window.addEventListener('resize', handleWindowResize);

      return () => {
        resizeObserver?.disconnect();
        window.removeEventListener('resize', handleWindowResize);
        xterm.dispose();
        xtermRef.current = null;
      };
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [sessionId, settings.terminal, getXtermTheme]);

    useEffect(() => {
      const xterm = xtermRef.current;
      if (!xterm) return;

      xterm.options.fontSize = settings.terminal.fontSize;
      xterm.options.fontFamily = `${settings.terminal.fontFamily}, Menlo, Monaco, Consolas, monospace`;
      xterm.options.cursorBlink = settings.terminal.cursorBlink;
      xterm.options.cursorStyle = settings.terminal.cursorStyle;
      xterm.options.scrollback = settings.terminal.scrollbackLines;
      xterm.options.theme = getXtermTheme();

      if (fitAddonRef.current) {
        fitAddonRef.current.fit();
      }
    }, [settings.terminal, settings.appearance.theme, getXtermTheme]);

    const currentTheme = themes.find((t) => t.name === settings.appearance.theme);
    const themeColors = currentTheme?.colors || themes[0].colors;
    const bgColor = themeColors.bg;

    return (
      <>
        <div
          ref={terminalRef}
          className="w-full h-full relative overflow-hidden"
          style={{ backgroundColor: bgColor }}
          onContextMenu={handleContextMenu}
        />

        <CompletionPopup
          items={completionState.items}
          selectedIndex={completionState.selectedIndex}
          position={completionState.position}
          visible={completionState.visible}
          onSelect={handleCompletionSelect}
          onSelectionChange={completionActions.setSelectedIndex}
          onClose={completionActions.hideCompletions}
          currentInput={completionState.currentInput}
        />

        {contextMenu.visible && (
          <div
            className="fixed z-50 min-w-[160px] py-1 rounded-lg shadow-lg border"
            style={{
              left: contextMenu.x,
              top: contextMenu.y,
              backgroundColor: themeColors.bgDark,
              borderColor: themeColors.bgHl,
              boxShadow: '0 4px 20px rgba(0, 0, 0, 0.3)',
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

        {completionState.ghostText && !completionState.visible && (
          <GhostTextOverlay
            ghostText={completionState.ghostText}
            terminalRef={terminalRef}
            xtermRef={xtermRef}
            themeColors={themeColors}
          />
        )}
      </>
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
    const termRect = terminalRef.current.getBoundingClientRect();
    const cellWidth = termRect.width / xterm.cols;
    const cellHeight = termRect.height / xterm.rows;
    const cursorX = xterm.buffer.active.cursorX;
    const cursorY = xterm.buffer.active.cursorY;

    setPosition({
      x: cursorX * cellWidth,
      y: cursorY * cellHeight,
    });

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
