import { memo, useCallback, useState, useEffect } from 'react';
import { AlertCircle, Loader2, Maximize2, Minimize2, Minus, Monitor, SquareTerminal, Wifi, X } from 'lucide-react';
import { cn } from '../lib/utils';

/**
 * Custom window titlebar replacing native decorations.
 * Uses Tauri's window API for minimize/maximize/close.
 * The drag region is handled via data-tauri-drag-region.
 */
export const TitleBar = memo(function TitleBar({
  activeSessionName,
  activeSessionType,
  activeSessionState,
}: {
  activeSessionName?: string;
  activeSessionType?: string;
  activeSessionState?: string;
}) {
  const [isMaximized, setIsMaximized] = useState(false);

  // Check initial maximized state and listen for changes
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const win = getCurrentWindow();
        setIsMaximized(await win.isMaximized());

        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen('tauri://resize', async () => {
          setIsMaximized(await win.isMaximized());
        });
      } catch {
        // Fallback for non-Tauri environment
      }
    };

    setup();
    return () => { unlisten?.(); };
  }, []);

  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().minimize();
    } catch { /* ignore */ }
  }, []);

  const handleMaximize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (await win.isMaximized()) {
        await win.unmaximize();
      } else {
        await win.maximize();
      }
    } catch { /* ignore */ }
  }, []);

  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().close();
    } catch { /* ignore */ }
  }, []);

  const sessionKind = activeSessionType === 'local' ? 'Local' : 'SSH';
  const stateLabel = activeSessionState && activeSessionState !== 'connected' ? activeSessionState : null;
  const StatusIcon = activeSessionState === 'connecting'
    ? Loader2
    : activeSessionState === 'error'
      ? AlertCircle
      : activeSessionType === 'local'
        ? Monitor
        : Wifi;

  return (
    <div
      className="titlebar-shell h-9 flex items-center justify-between select-none shrink-0"
    >
      {/* Left: App branding + drag region */}
      <div
        className="flex items-center gap-3 pl-3 flex-1 h-full min-w-0"
        data-tauri-drag-region
      >
        <span className="titlebar-brand" data-tauri-drag-region>
          <SquareTerminal className="w-4 h-4" aria-hidden="true" />
        </span>
        <span
          className="text-xs font-semibold text-tokyo-blue"
          data-tauri-drag-region
        >
          VibeShell
        </span>

        {/* Session info */}
        {activeSessionName && (
          <div
            className="titlebar-session min-w-0"
            data-tauri-drag-region
          >
            <StatusIcon
              className={cn(
                'w-3.5 h-3.5 flex-shrink-0',
                activeSessionState === 'connecting' && 'animate-spin text-tokyo-yellow',
                activeSessionState === 'error' && 'text-tokyo-red',
                activeSessionState === 'connected' && activeSessionType === 'local' && 'text-tokyo-blue',
                activeSessionState === 'connected' && activeSessionType !== 'local' && 'text-tokyo-green',
                (!activeSessionState || activeSessionState === 'disconnected') && 'text-tokyo-comment'
              )}
              aria-hidden="true"
            />
            <span className="flex-shrink-0 text-tokyo-comment">{sessionKind}</span>
            <span className="truncate text-tokyo-fg">{activeSessionName}</span>
            {stateLabel && <span className="flex-shrink-0 text-tokyo-comment">({stateLabel})</span>}
          </div>
        )}
      </div>

      {/* Right: Window controls */}
      <div className="flex items-center h-full">
        {/* Minimize */}
        <button
          className="window-control"
          onClick={handleMinimize}
          aria-label="Minimize"
        >
          <Minus className="w-3.5 h-3.5" aria-hidden="true" />
        </button>

        {/* Maximize / Restore */}
        <button
          className="window-control"
          onClick={handleMaximize}
          aria-label={isMaximized ? 'Restore' : 'Maximize'}
        >
          {isMaximized ? (
            <Minimize2 className="w-3.5 h-3.5" aria-hidden="true" />
          ) : (
            <Maximize2 className="w-3.5 h-3.5" aria-hidden="true" />
          )}
        </button>

        {/* Close */}
        <button
          className="window-control window-control-danger"
          onClick={handleClose}
          aria-label="Close"
        >
          <X className="w-3.5 h-3.5" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
});
