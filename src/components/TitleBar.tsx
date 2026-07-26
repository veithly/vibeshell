import { memo, useCallback, useState, useEffect } from 'react';
import { AlertCircle, Code2, Loader2, Maximize2, Minimize2, Minus, Monitor, Wifi, X } from 'lucide-react';
import { cn } from '../lib/utils';
import { useRuntimeCapabilitiesStore } from '../stores/runtimeCapabilitiesStore';

const MACOS_FULLSCREEN_TRANSITION_MS = 700;

function waitForMacosFullscreenTransition(): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, MACOS_FULLSCREEN_TRANSITION_MS));
}

/**
 * Custom window titlebar replacing native decorations.
 * Uses Tauri's window API for minimize/maximize/close.
 * The drag region is handled via data-tauri-drag-region.
 */
export const TitleBar = memo(function TitleBar({
  activeSessionName,
  activeSessionType,
  activeSessionState,
  activeSessionPurpose,
}: {
  activeSessionName?: string;
  activeSessionType?: string;
  activeSessionState?: string;
  activeSessionPurpose?: string;
}) {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const platform = useRuntimeCapabilitiesStore((state) => state.capabilities.platform);
  const supportsWindowControls = useRuntimeCapabilitiesStore(
    (state) => state.capabilities.windowControls
  );
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);

  useEffect(() => {
    void loadRuntimeCapabilities();
  }, [loadRuntimeCapabilities]);

  // Keep the custom controls synchronized with native window transitions.
  useEffect(() => {
    if (!supportsWindowControls) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const win = getCurrentWindow();
        const updateWindowState = async () => {
          const [maximized, fullscreen] = await Promise.all([
            win.isMaximized(),
            win.isFullscreen(),
          ]);
          if (cancelled) return;
          setIsMaximized(maximized);
          setIsFullscreen(fullscreen);
        };
        await updateWindowState();

        const { listen } = await import('@tauri-apps/api/event');
        const stop = await listen('tauri://resize', updateWindowState);
        if (cancelled) {
          // Effect tore down while we were awaiting listen(); release it now.
          stop();
        } else {
          unlisten = stop;
        }
      } catch {
        // Fallback for non-Tauri environment
      }
    };

    setup();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [supportsWindowControls]);

  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (platform === 'macos' && await win.isFullscreen()) {
        await win.setFullscreen(false);
        setIsFullscreen(false);
        await waitForMacosFullscreenTransition();
      }
      await win.minimize();
    } catch (error) {
      console.error('[TitleBar] Failed to minimize window:', error);
    }
  }, [platform]);

  const handleWindowExpand = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (platform === 'macos') {
        const nextFullscreen = !(await win.isFullscreen());
        await win.setFullscreen(nextFullscreen);
        setIsFullscreen(await win.isFullscreen());
      } else {
        await win.toggleMaximize();
        setIsMaximized(await win.isMaximized());
      }
    } catch (error) {
      console.error('[TitleBar] Failed to expand window:', error);
    }
  }, [platform]);

  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const win = getCurrentWindow();
      if (platform === 'macos' && await win.isFullscreen()) {
        await win.setFullscreen(false);
        setIsFullscreen(false);
        await waitForMacosFullscreenTransition();
      }
      await win.close();
    } catch (error) {
      console.error('[TitleBar] Failed to close window:', error);
    }
  }, [platform]);

  const windowControls = !supportsWindowControls ? null : platform === 'macos' ? (
    <div className="window-controls-macos" aria-label="Window controls">
      <button className="window-control-macos macos-close" onClick={handleClose} aria-label="Close">
        <X aria-hidden="true" />
      </button>
      <button className="window-control-macos macos-minimize" onClick={handleMinimize} aria-label="Minimize">
        <Minus aria-hidden="true" />
      </button>
      <button
        className="window-control-macos macos-maximize"
        onClick={handleWindowExpand}
        aria-label={isFullscreen ? 'Exit fullscreen' : 'Enter fullscreen'}
      >
        {isFullscreen ? <Minimize2 aria-hidden="true" /> : <Maximize2 aria-hidden="true" />}
      </button>
    </div>
  ) : (
    <div className={cn('window-controls-desktop', platform === 'windows' ? 'is-windows' : 'is-linux')}>
      <button className="window-control-desktop" onClick={handleMinimize} aria-label="Minimize">
        <Minus aria-hidden="true" />
      </button>
      <button className="window-control-desktop" onClick={handleWindowExpand} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
        {isMaximized ? <Minimize2 aria-hidden="true" /> : <Maximize2 aria-hidden="true" />}
      </button>
      <button className="window-control-desktop window-control-close" onClick={handleClose} aria-label="Close">
        <X aria-hidden="true" />
      </button>
    </div>
  );

  const sessionKind = activeSessionPurpose === 'coding_agent'
    ? 'Agent'
    : activeSessionType === 'local' ? 'Local' : 'SSH';
  const stateLabel = activeSessionState && activeSessionState !== 'connected' ? activeSessionState : null;
  const StatusIcon = activeSessionState === 'connecting'
    ? Loader2
    : activeSessionState === 'error'
      ? AlertCircle
      : activeSessionPurpose === 'coding_agent'
        ? Code2
        : activeSessionType === 'local'
          ? Monitor
          : Wifi;

  return (
    <div
      className={cn('titlebar-shell h-9 flex items-center justify-between select-none shrink-0', `platform-${platform}`)}
    >
      {platform === 'macos' && windowControls}
      {/*
        Drag region is intentionally split into narrow gutter strips instead of
        covering the whole titlebar. A single large data-tauri-drag-region makes
        Tauri's native startDragging() capture mouse events over interactive
        elements, which eats rapid clicks and makes the UI feel frozen. Keep the
        brand, session info, and any future controls outside the drag surface.
      */}
      <div className="flex items-center gap-3 flex-1 h-full min-w-0">
        <div
          className="titlebar-drag-gutter h-full"
          data-tauri-drag-region
          aria-hidden="true"
        />
        <span className={cn('titlebar-brand', platform === 'macos' ? '' : 'pl-1')}>
          <img src="/app-icon.svg" className="h-5 w-5" alt="" aria-hidden="true" />
        </span>
        <span className="text-xs font-semibold text-tokyo-fg">VibeShell</span>

        {/* Session info */}
        {activeSessionName && (
          <div className="titlebar-session min-w-0">
            <StatusIcon
              className={cn(
                'w-3.5 h-3.5 flex-shrink-0',
                activeSessionState === 'connecting' && 'animate-spin text-tokyo-yellow',
                activeSessionState === 'error' && 'text-tokyo-red',
                activeSessionState === 'connected' && 'text-tokyo-cyan',
                (!activeSessionState || activeSessionState === 'disconnected') && 'text-tokyo-comment'
              )}
              aria-hidden="true"
            />
            <span className="flex-shrink-0 text-tokyo-comment">{sessionKind}</span>
            <span className="truncate text-tokyo-fg">{activeSessionName}</span>
            {stateLabel && <span className="flex-shrink-0 text-tokyo-comment">({stateLabel})</span>}
          </div>
        )}

        <div
          className="titlebar-drag-gutter flex-1 h-full"
          data-tauri-drag-region
          aria-hidden="true"
        />
      </div>

      {platform !== 'macos' && windowControls}
    </div>
  );
});
