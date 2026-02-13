import { memo, useCallback, useState, useEffect } from 'react';
import { useSettingsStore, themes } from '../stores/settingsStore';

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
  const { settings } = useSettingsStore();
  const currentTheme = themes.find(t => t.name === settings.appearance.theme);
  const colors = currentTheme?.colors || themes[0].colors;

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

  return (
    <div
      className="h-9 flex items-center justify-between select-none shrink-0"
      style={{ backgroundColor: colors.bgDark, borderBottom: `1px solid ${colors.bgHl}` }}
    >
      {/* Left: App branding + drag region */}
      <div
        className="flex items-center gap-2 pl-3 flex-1 h-full"
        data-tauri-drag-region
      >
        {/* Logo */}
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" style={{ flexShrink: 0 }}>
          <path d="M2 4l4 4-4 4" stroke={colors.accent} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          <path d="M8 12h6" stroke={colors.accent} strokeWidth="2" strokeLinecap="round" />
        </svg>
        <span
          className="text-xs font-semibold tracking-wide"
          style={{ color: colors.accent }}
          data-tauri-drag-region
        >
          VibeShell
        </span>

        {/* Session info */}
        {activeSessionName && (
          <span
            className="text-xs ml-2 truncate"
            style={{ color: colors.fgDark }}
            data-tauri-drag-region
          >
            {activeSessionType === 'local' ? 'Local' : 'SSH'}
            {' \u2022 '}
            {activeSessionName}
            {activeSessionState && activeSessionState !== 'connected' && (
              <span style={{ color: colors.fgDark, opacity: 0.6 }}> ({activeSessionState})</span>
            )}
          </span>
        )}
      </div>

      {/* Right: Window controls */}
      <div className="flex items-center h-full">
        {/* Minimize */}
        <button
          className="h-full w-11 flex items-center justify-center transition-colors duration-100"
          style={{ color: colors.fgDark }}
          onMouseEnter={e => { e.currentTarget.style.backgroundColor = colors.bgHl; e.currentTarget.style.color = colors.fg; }}
          onMouseLeave={e => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = colors.fgDark; }}
          onClick={handleMinimize}
          aria-label="Minimize"
        >
          <svg width="10" height="1" viewBox="0 0 10 1">
            <rect width="10" height="1" fill="currentColor" />
          </svg>
        </button>

        {/* Maximize / Restore */}
        <button
          className="h-full w-11 flex items-center justify-center transition-colors duration-100"
          style={{ color: colors.fgDark }}
          onMouseEnter={e => { e.currentTarget.style.backgroundColor = colors.bgHl; e.currentTarget.style.color = colors.fg; }}
          onMouseLeave={e => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = colors.fgDark; }}
          onClick={handleMaximize}
          aria-label={isMaximized ? 'Restore' : 'Maximize'}
        >
          {isMaximized ? (
            // Restore icon (two overlapping rectangles)
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <rect x="2" y="0" width="8" height="8" rx="0.5" stroke="currentColor" strokeWidth="1" fill="none" />
              <rect x="0" y="2" width="8" height="8" rx="0.5" stroke="currentColor" strokeWidth="1" fill={colors.bgDark} />
            </svg>
          ) : (
            // Maximize icon (single rectangle)
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
              <rect x="0.5" y="0.5" width="9" height="9" rx="0.5" stroke="currentColor" strokeWidth="1" />
            </svg>
          )}
        </button>

        {/* Close */}
        <button
          className="h-full w-11 flex items-center justify-center transition-colors duration-100"
          style={{ color: colors.fgDark }}
          onMouseEnter={e => { e.currentTarget.style.backgroundColor = '#c42b1c'; e.currentTarget.style.color = '#ffffff'; }}
          onMouseLeave={e => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = colors.fgDark; }}
          onClick={handleClose}
          aria-label="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        </button>
      </div>
    </div>
  );
});
