import { memo, useCallback, useState, useEffect, type ReactNode } from 'react';
import { Maximize2, Minimize2, Minus, X } from 'lucide-react';
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
  children,
}: {
  activeSessionName?: string;
  children?: ReactNode;
}) {
  const [isMaximized, setIsMaximized] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const platform = useRuntimeCapabilitiesStore((state) => state.capabilities.platform);
  const supportsWindowControls = useRuntimeCapabilitiesStore(
    (state) => state.capabilities.windowControls
  );
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);

  useEffect(() => {
    document.documentElement.dataset.windowFullscreen = String(isFullscreen || isMaximized);
  }, [isFullscreen, isMaximized]);

  useEffect(() => {
    void loadRuntimeCapabilities();
  }, [loadRuntimeCapabilities]);

  // Keep the custom controls synchronized with native window transitions.
  useEffect(() => {
    if (!supportsWindowControls) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reading = false;

    const setup = async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        const win = getCurrentWindow();
        const updateWindowState = async () => {
          if (cancelled || reading) return;
          reading = true;
          try {
            // The macOS green control represents fullscreen, not AppKit's
            // zoomed state. Avoid expensive isZoomed/standardFrame probing.
            if (platform === 'macos') {
              const fullscreen = await win.isFullscreen();
              if (!cancelled) setIsFullscreen(fullscreen);
            } else {
              const maximized = await win.isMaximized();
              if (!cancelled) setIsMaximized(maximized);
            }
          } finally { reading = false; }
        };
        await updateWindowState();

        const { listen } = await import('@tauri-apps/api/event');
        const stop = await listen('tauri://resize', () => {
          if (cancelled || reading) return;
          clearTimeout(timer);
          timer = setTimeout(() => { void updateWindowState().catch(console.error); }, 200);
        });
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
      clearTimeout(timer);
      unlisten?.();
    };
  }, [supportsWindowControls, platform]);

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

  return (
    <div
      className={cn('titlebar-shell flex items-center select-none shrink-0', `platform-${platform}`)}
    >
      {platform === 'macos' && windowControls}
      {/* Only empty gutters drag the native window; tabs retain their own gestures. */}
      <div className="titlebar-drag-gutter h-full shrink-0" data-tauri-drag-region aria-hidden="true" />
      {children ?? (
        <div className="flex h-full min-w-0 flex-1 items-center text-sm text-tokyo-fg" data-tauri-drag-region>
          <span className="pointer-events-none truncate">{activeSessionName}</span>
        </div>
      )}
      {platform !== 'macos' && windowControls}
    </div>
  );
});
