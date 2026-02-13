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

/**
 * PERFORMANCE: Throttle function to limit how often a function can be called
 */
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
      // Schedule a trailing call
      timeoutId = setTimeout(() => {
        lastCall = Date.now();
        timeoutId = null;
        func(...args);
      }, limit - (now - lastCall));
    }
  };
}

/**
 * PERFORMANCE: Zustand selector for session type lookup.
 * Only re-renders when the specific session's type changes, not on every sessions array mutation.
 */
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

/**
 * Terminal handle exposed to parent components
 */
export interface TerminalHandle {
  /** Write data to the terminal */
  write: (data: string | Uint8Array) => void;
  /** Write a line to the terminal */
  writeln: (data: string) => void;
  /** Clear the terminal */
  clear: () => void;
  /** Focus the terminal */
  focus: () => void;
  /** Get current terminal dimensions */
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

    // Input line buffer for completion
    const inputBufferRef = useRef<string>('');
    const cursorPositionRef = useRef<number>(0);

    // PERFORMANCE: Use sendInputFast for typing (fire-and-forget with batching)
    // Use sendInput only when we need confirmation (e.g., completion insertion)
    // NOTE: We grab stable store action references once; they don't change across renders.
    const sendInput = useSessionStore((s) => s.sendInput);
    const sendInputFast = useSessionStore((s) => s.sendInputFast);
    const resizeSession = useSessionStore((s) => s.resizeSession);
    const sendLocalShellInput = useSessionStore((s) => s.sendLocalShellInput);
    const sendLocalShellInputFast = useSessionStore((s) => s.sendLocalShellInputFast);
    const resizeLocalShellSession = useSessionStore((s) => s.resizeLocalShellSession);
    const { settings } = useSettingsStore();

    // PERFORMANCE: Use selector to only re-render when this session's type changes,
    // instead of subscribing to the entire sessions array.
    const sessionType = useSessionType(sessionId);
    const isLocalShell = sessionType === 'local';

    // Select appropriate functions based on session type
    const sendInputForSession = isLocalShell ? sendLocalShellInput : sendInput;
    const sendInputFastForSession = isLocalShell ? sendLocalShellInputFast : sendInputFast;
    const resizeSessionForSession = isLocalShell ? resizeLocalShellSession : resizeSession;

    // PERFORMANCE: Store session-type-dependent functions in refs so the main xterm
    // useEffect doesn't re-run when session type changes (which would recreate the terminal).
    const sendInputFastRef = useRef(sendInputFastForSession);
    const sendInputForSessionRef = useRef(sendInputForSession);
    const resizeForSessionRef = useRef(resizeSessionForSession);
    const onDataRef = useRef(onData);

    useEffect(() => { sendInputFastRef.current = sendInputFastForSession; }, [sendInputFastForSession]);
    useEffect(() => { sendInputForSessionRef.current = sendInputForSession; }, [sendInputForSession]);
    useEffect(() => { resizeForSessionRef.current = resizeSessionForSession; }, [resizeSessionForSession]);
    useEffect(() => { onDataRef.current = onData; }, [onData]);

    // Completion state
    const [completionState, completionActions] = useCompletion();

    // Context menu state
    const [contextMenu, setContextMenu] = useState<{
      visible: boolean;
      x: number;
      y: number;
    }>({
      visible: false,
      x: 0,
      y: 0,
    });

    // Use refs to access completion state/actions inside event handlers
    // This prevents the terminal from being recreated when completion state changes
    const completionStateRef = useRef(completionState);
    const completionActionsRef = useRef(completionActions);

    // Keep refs in sync with current values
    useEffect(() => {
      completionStateRef.current = completionState;
    }, [completionState]);

    useEffect(() => {
      completionActionsRef.current = completionActions;
    }, [completionActions]);

    // PERFORMANCE: Removed verbose console.log from render path

    // Expose terminal methods to parent
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

    // Handle terminal resize - PERFORMANCE: Uses ref for stable function identity
    // The resize handler reads sessionId from a ref to avoid recreating on sessionId change
    const sessionIdRef = useRef(sessionId);
    useEffect(() => { sessionIdRef.current = sessionId; }, [sessionId]);

    // PERFORMANCE: Single stable throttled resize handler that uses refs.
    // Created once and never recreated.
    const handleTerminalResize = useRef(
      throttle((cols: number, rows: number) => {
        const sid = sessionIdRef.current;
        if (sid) {
          resizeForSessionRef.current(sid, cols, rows);
        }
      }, 100)
    ).current;

    /**
     * Handle copy operation
     */
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

    /**
     * Handle paste operation
     * PERFORMANCE: Uses refs for sessionId and sendInputForSession to keep stable identity
     */
    const handlePaste = useCallback(async () => {
      const sid = sessionIdRef.current;
      if (!sid) return;

      try {
        const text = await navigator.clipboard.readText();
        if (text) {
          // Handle multi-line paste by replacing newlines with carriage returns
          const processedText = text.replace(/\r?\n/g, '\r');
          sendInputForSessionRef.current(sid, processedText);

          // Conservative strategy for paste: backend/shell may transform line state
          // (bracketed paste, multiline handling, shell edits), so reset predicted buffer.
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          completionActionsRef.current.hideCompletions();
          completionActionsRef.current.clearGhostText();
        }
      } catch (err) {
        console.error('[Terminal] Failed to paste:', err);
      }
    }, []);

    /**
     * Handle context menu
     */
    const handleContextMenu = useCallback((e: React.MouseEvent) => {
      e.preventDefault();
      setContextMenu({
        visible: true,
        x: e.clientX,
        y: e.clientY,
      });
    }, []);

    /**
     * Close context menu
     */
    const closeContextMenu = useCallback(() => {
      setContextMenu(prev => ({ ...prev, visible: false }));
    }, []);

    // Close context menu on click outside
    useEffect(() => {
      if (!contextMenu.visible) return;

      const handleClick = () => closeContextMenu();
      window.addEventListener('click', handleClick);
      return () => window.removeEventListener('click', handleClick);
    }, [contextMenu.visible, closeContextMenu]);

    // Set up keyboard shortcuts for copy/paste
    useEffect(() => {
      const handleKeyDown = (e: KeyboardEvent) => {
        // Ctrl+C - Copy (when text is selected) or send interrupt
        if ((e.ctrlKey || e.metaKey) && e.key === 'c') {
          const xterm = xtermRef.current;
          if (xterm && xterm.hasSelection()) {
            e.preventDefault();
            handleCopy();
          }
          // If no selection, let it through as Ctrl+C interrupt
        }

        // Ctrl+Shift+C - Always copy
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'C') {
          e.preventDefault();
          handleCopy();
        }

        // Ctrl+V or Ctrl+Shift+V - Paste
        if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || (e.shiftKey && e.key === 'V'))) {
          e.preventDefault();
          handlePaste();
        }
      };

      window.addEventListener('keydown', handleKeyDown);
      return () => window.removeEventListener('keydown', handleKeyDown);
    }, [handleCopy, handlePaste]);

    /**
     * Handle completion selection - insert the selected text.
     * PERFORMANCE: Uses refs for sessionId and sendInput to keep stable identity
     */
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

      // Determine what to insert based on current input
      const currentInput = inputBufferRef.current;
      const parts = currentInput.trim().split(/\s+/);

      let textToInsert: string;

      if (item.isHistory) {
        // For history items, replace the entire line
        // First, clear the current input
        const backspaces = '\x7f'.repeat(currentInput.length);
        sendFn(sid, backspaces);
        textToInsert = item.text;
      } else if (parts.length >= 2) {
        // Completing a subcommand - only insert the subcommand
        const lastPart = parts[parts.length - 1];
        const backspaces = '\x7f'.repeat(lastPart.length);
        sendFn(sid, backspaces);
        textToInsert = completionText;
      } else {
        // Completing a command - replace the current input
        const backspaces = '\x7f'.repeat(currentInput.length);
        sendFn(sid, backspaces);
        textToInsert = completionText;
      }

      // Send the completion text
      sendFn(sid, textToInsert);

      // Update buffer
      if (item.isHistory) {
        inputBufferRef.current = item.text;
      } else if (parts.length >= 2) {
        parts[parts.length - 1] = completionText;
        inputBufferRef.current = parts.join(' ');
      } else {
        inputBufferRef.current = completionText;
      }

      // Hide completions
      actions.hideCompletions();

      // Focus terminal
      xtermRef.current?.focus();
    }, []);

    // Ref for handleCompletionSelect so it can be used inside onData without stale closures
    const handleCompletionSelectRef = useRef(handleCompletionSelect);
    useEffect(() => {
      handleCompletionSelectRef.current = handleCompletionSelect;
    }, [handleCompletionSelect]);

    // Set up event listener for session output
    // PERFORMANCE: Removed verbose logging from hot path (output events)
    useEffect(() => {
      if (!sessionId) {
        return;
      }

      setConnectionStatus('initializing');
      receivedDataRef.current = false;

      let unlisten: (() => void) | null = null;

      // Event payload type from backend
      interface SessionOutputEvent {
        session_id?: string; // SSH events
        sessionId?: string; // local shell events (camelCase)
        data: number[];
      }

      const setupListener = async () => {
        try {
          const { listen } = await import('@tauri-apps/api/event');

          // Listen for the global session-output event
          // PERFORMANCE: Minimal processing in hot path - just write to xterm
          unlisten = await listen<SessionOutputEvent>('session-output', (event) => {
            // Filter by session ID to only process output for this terminal
            const payloadSessionId = event.payload?.session_id ?? event.payload?.sessionId;
            if (payloadSessionId === sessionId && event.payload?.data) {
              if (!receivedDataRef.current) {
                receivedDataRef.current = true;
                setConnectionStatus('receiving');
              }

              if (xtermRef.current) {
                // Convert number array to Uint8Array and write directly
                xtermRef.current.write(new Uint8Array(event.payload.data));
              }
            }
          });

          eventUnlistenRef.current = unlisten;
          setConnectionStatus('listening');

          // Write a status message to the terminal
          if (xtermRef.current) {
            if (isLocalShell) {
              xtermRef.current.writeln('\x1b[1;32m[Ready]\x1b[0m Local shell initialized.');
            } else {
              xtermRef.current.writeln('\x1b[1;32m[Connected]\x1b[0m Waiting for SSH output...');
            }
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
      };
    }, [sessionId, isLocalShell]);

    // Helper function to get theme colors for XTerm
    const getXtermTheme = useCallback(() => {
      const currentTheme = themes.find(t => t.name === settings.appearance.theme);
      // Default terminal colors based on selected theme
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

      // PERFORMANCE: XTerm configuration optimized for responsiveness
      const xterm = new XTerm({
        theme: getXtermTheme(),
        fontSize: settings.terminal.fontSize,
        fontFamily: `${settings.terminal.fontFamily}, Menlo, Monaco, Consolas, monospace`,
        cursorBlink: settings.terminal.cursorBlink,
        cursorStyle: settings.terminal.cursorStyle,
        scrollback: settings.terminal.scrollbackLines,
        // PERFORMANCE OPTIONS:
        fastScrollModifier: 'alt', // Hold Alt for fast scrolling
        fastScrollSensitivity: 5,  // Fast scroll multiplier
        smoothScrollDuration: 0,   // Disable smooth scroll for instant response
        allowProposedApi: true,    // Enable latest features
        // Rendering optimizations
        drawBoldTextInBrightColors: true,
        minimumContrastRatio: 1, // Disable contrast adjustment for perf
      });

      const fitAddon = new FitAddon();
      const webLinksAddon = new WebLinksAddon();

      xterm.loadAddon(fitAddon);
      xterm.loadAddon(webLinksAddon);

      // PERFORMANCE: Try to load WebGL addon for GPU acceleration
      // Falls back to canvas renderer if WebGL not available
      try {
        const webglAddon = new WebglAddon();
        webglAddon.onContextLoss(() => {
          // Gracefully handle context loss by disposing the addon
          webglAddon.dispose();
        });
        xterm.loadAddon(webglAddon);
      } catch {
        // WebGL not available, continue with canvas renderer
        console.info('[Terminal] WebGL not available, using canvas renderer');
      }

      xterm.open(terminalRef.current);
      fitAddon.fit();

      xtermRef.current = xterm;
      fitAddonRef.current = fitAddon;

      // Ctrl+Space: toggle suggestions popup
      // Intercepted at raw keyboard level to avoid IME conflicts
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
          return false; // Prevent xterm from processing
        }
        return true;
      });

      // Handle user input - send to backend and notify parent
      // PERFORMANCE: Uses refs for all external dependencies to avoid terminal recreation.
      // sendInputFastRef / sendInputForSessionRef / onDataRef are kept in sync via useEffect above.
      xterm.onData((data) => {
        // Access completion state/actions via refs to avoid dependency issues
        const compState = completionStateRef.current;
        const compActions = completionActionsRef.current;

        // Handle Tab key:
        // - If completion popup is visible, accept the selected suggestion
        // - Otherwise, forward to shell for native path completion
        if (data === '\t') {
          if (compState.visible) {
            const selectedItem = compActions.getSelectedItem();
            if (selectedItem) {
              handleCompletionSelectRef.current(selectedItem);
              return; // Don't send Tab to shell
            }
            compActions.hideCompletions();
          }
          compActions.clearGhostText();
        }

        // Handle Escape key to close completions
        if (data === '\x1b' && compState.visible) {
          compActions.hideCompletions();
          return;
        }

        // Handle arrow keys when completion popup is visible
        if (compState.visible) {
          if (data === '\x1b[A') { // Up arrow
            compActions.selectPrev();
            return;
          }
          if (data === '\x1b[B') { // Down arrow
            compActions.selectNext();
            return;
          }
        }

        // Right Arrow: accept ghost text suggestion (fish shell / zsh-autosuggestions style)
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

        // Keep local input prediction conservative: ANSI cursor/edit sequences can mutate
        // shell line state in ways we cannot reliably mirror client-side.
        const isAnsiCursorOrEditSequence = /^(?:\x1b\[[0-9;?]*[~A-Za-z]|\x1bO[0-9A-Za-z])$/.test(data);
        if (isAnsiCursorOrEditSequence) {
          resetPredictedInputState();
        }

        // Conservative downgrade for control chars that are difficult to sync locally
        // (examples: Ctrl+A/Ctrl+E and similar readline/navigation edits).
        const shouldResetForControlChar = [
          '\t',   // Tab: shell completion may alter line state
          '\x01', // Ctrl+A: move to line start
          '\x02', // Ctrl+B: backward char
          '\x04', // Ctrl+D: delete/EOF
          '\x05', // Ctrl+E: move to line end
          '\x06', // Ctrl+F: forward char
          '\x0b', // Ctrl+K: kill to end
          '\x0c', // Ctrl+L: clear screen/redraw
          '\x0e', // Ctrl+N: next history
          '\x10', // Ctrl+P: previous history
          '\x17', // Ctrl+W: delete previous word
        ].includes(data);

        if (shouldResetForControlChar) {
          resetPredictedInputState();
        }

        // Handle Enter key - add command to history and reset buffer
        if (data === '\r' || data === '\n') {
          const command = inputBufferRef.current.trim();
          if (command) {
            compActions.addToHistory(command);
          }
          inputBufferRef.current = '';
          cursorPositionRef.current = 0;
          compActions.hideCompletions();
          compActions.clearGhostText();
        }
        // Handle Backspace - update buffer and completions
        else if (data === '\x7f' || data === '\b') {
          if (inputBufferRef.current.length > 0) {
            inputBufferRef.current = inputBufferRef.current.slice(0, -1);
            cursorPositionRef.current = Math.max(0, cursorPositionRef.current - 1);
          }
          // Update completions or hide if input becomes empty
          if (!inputBufferRef.current.trim()) {
            compActions.hideCompletions();
            compActions.clearGhostText();
          } else if (compState.visible) {
            compActions.updateCompletions(inputBufferRef.current);
          } else {
            // Refresh ghost text when popup is not visible
            compActions.autoTrigger(inputBufferRef.current, { x: 0, y: 0 });
          }
        }
        // Handle Ctrl+C / Ctrl+U - reset buffer
        else if (data === '\x03' || data === '\x15') {
          resetPredictedInputState();
        }
        // Handle printable single-character input (including non-ASCII)
        else if (data.length === 1 && !/[\x00-\x1F\x7F]/.test(data)) {
          inputBufferRef.current += data;
          cursorPositionRef.current += 1;

          // Auto-trigger completions while typing
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

            // PERFORMANCE: Removed console.debug from hot typing path

            // When popup is open, update filtered suggestions inline;
            // otherwise trigger the popup
            if (compState.visible) {
              compActions.updateCompletions(inputBufferRef.current);
            } else {
              compActions.autoTrigger(inputBufferRef.current, position);
            }
          }
        }
        // Multi-char non-ANSI payloads (e.g. paste bursts/IME text): avoid stale concatenation.
        else if (data.length > 1 && !isAnsiCursorOrEditSequence) {
          resetPredictedInputState();
        }

        // PERFORMANCE: Send input using fire-and-forget with batching via ref
        // This minimizes latency by not awaiting the IPC result
        const sid = sessionIdRef.current;
        if (sid) {
          sendInputFastRef.current(sid, data);
        }
        // Also notify parent component (for local echo or other handling)
        onDataRef.current?.(data);
      });

      // Handle terminal resize events
      // PERFORMANCE: Using throttled resize handler to avoid excessive IPC calls
      xterm.onResize(({ cols, rows }) => {
        handleTerminalResize(cols, rows);
      });

      // Welcome message with session info
      xterm.writeln('\x1b[1;34mVibeShell Terminal\x1b[0m');
      if (sessionId) {
        xterm.writeln(`\x1b[90mSession: ${sessionId}\x1b[0m`);
        if (isLocalShell) {
          xterm.writeln('\x1b[1;33m[Starting...]\x1b[0m Initializing local shell...');
        } else {
          xterm.writeln('\x1b[1;33m[Connecting...]\x1b[0m Establishing SSH connection...');
        }
      } else {
        xterm.writeln('\x1b[90mNo session - type to test local echo\x1b[0m');
      }
      xterm.writeln('');

      // Send initial resize
      handleTerminalResize(xterm.cols, xterm.rows);

      // PERFORMANCE: Use ResizeObserver instead of window resize for precise container tracking.
      // This handles cases where the terminal container resizes without a window resize
      // (e.g., sidebar collapse, panel drag, CSS transitions).
      let resizeObserver: ResizeObserver | null = null;
      if (terminalRef.current) {
        resizeObserver = new ResizeObserver(() => {
          // Use requestAnimationFrame to batch with rendering
          requestAnimationFrame(() => {
            if (fitAddonRef.current) {
              fitAddonRef.current.fit();
            }
          });
        });
        resizeObserver.observe(terminalRef.current);
      }

      // Also listen for window resize as a fallback (e.g., maximise/restore)
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
    // PERFORMANCE: Minimal dependency array — all mutable state is accessed via refs.
    // Only sessionId and settings.terminal (for initial xterm config) trigger terminal recreation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [sessionId, settings.terminal, getXtermTheme]);

    // Update terminal options when settings change (without recreating the terminal)
    useEffect(() => {
      const xterm = xtermRef.current;
      if (!xterm) return;

      // Update terminal options dynamically
      xterm.options.fontSize = settings.terminal.fontSize;
      xterm.options.fontFamily = `${settings.terminal.fontFamily}, Menlo, Monaco, Consolas, monospace`;
      xterm.options.cursorBlink = settings.terminal.cursorBlink;
      xterm.options.cursorStyle = settings.terminal.cursorStyle;
      xterm.options.scrollback = settings.terminal.scrollbackLines;
      xterm.options.theme = getXtermTheme();

      // Refit after font changes
      if (fitAddonRef.current) {
        fitAddonRef.current.fit();
      }
    }, [settings.terminal, settings.appearance.theme, getXtermTheme]);

    // Get background color from current theme
    const currentTheme = themes.find(t => t.name === settings.appearance.theme);
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

        {/* Completion Popup */}
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

        {/* Context Menu */}
        {contextMenu.visible && (
          <div
            className="fixed z-50 min-w-[160px] py-1 rounded-lg shadow-lg border"
            style={{
              left: contextMenu.x,
              top: contextMenu.y,
              backgroundColor: themeColors.bgDark,
              borderColor: themeColors.bgHl,
              boxShadow: `0 4px 20px rgba(0, 0, 0, 0.3)`,
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

        {/* Ghost Text Overlay */}
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

/**
 * Ghost text overlay component that shows inline suggestions
 */
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

    // Get cell dimensions
    const cellWidth = termRect.width / xterm.cols;
    const cellHeight = termRect.height / xterm.rows;

    // Get cursor position within terminal
    const cursorX = xterm.buffer.active.cursorX;
    const cursorY = xterm.buffer.active.cursorY;

    // Calculate position relative to terminal container
    setPosition({
      x: cursorX * cellWidth,
      y: cursorY * cellHeight,
    });

    // Get font size from xterm options
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
