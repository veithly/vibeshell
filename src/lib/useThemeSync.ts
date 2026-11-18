import { useEffect } from 'react';
import { useCustomTheme } from './useCustomTheme';
import { useSettingsStore, themes, isLightTheme, normalizeTerminalBackgroundOpacity, normalizeWindowMaterialOpacity } from '../stores/settingsStore';
import { useRuntimeCapabilitiesStore } from '../stores/runtimeCapabilitiesStore';

/**
 * Applies the selected theme to the document root and keeps a `system` theme
 * mode in sync with the OS. Shared by the main window and detached windows so
 * torn-out tabs look identical to the main app.
 */
export function useThemeSync(): void {
  const settings = useSettingsStore((state) => state.settings);
  const initializeSettings = useSettingsStore((state) => state.initializeSettings);
  const updateAppearanceSettings = useSettingsStore((state) => state.updateAppearanceSettings);
  const platform = useRuntimeCapabilitiesStore((state) => state.capabilities.platform);
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);

  useEffect(() => {
    document.documentElement.dataset.nativeGlass = String(platform === 'macos' && '__TAURI_INTERNALS__' in window);
  }, [platform]);

  useEffect(() => {
    const opacity = normalizeTerminalBackgroundOpacity(settings.appearance.terminalBackgroundOpacity);
    document.documentElement.style.setProperty('--vibe-terminal-mask', `${opacity * 100}%`);
  }, [settings.appearance.terminalBackgroundOpacity]);

  useEffect(() => {
    const opacity = normalizeWindowMaterialOpacity(settings.appearance.windowMaterialOpacity);
    document.documentElement.style.setProperty('--vibe-window-mask', `${opacity * 100}%`);
  }, [settings.appearance.windowMaterialOpacity]);

  useEffect(() => {
    void initializeSettings();
    void loadRuntimeCapabilities();
  }, [initializeSettings, loadRuntimeCapabilities]);

  useEffect(() => {
    if (settings.appearance.themeMode !== 'system' || typeof window === 'undefined' || !window.matchMedia) {
      return;
    }

    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const applySystemTheme = () => {
      const nextTheme = mediaQuery.matches
        ? settings.appearance.darkTheme
        : settings.appearance.lightTheme;
      if (settings.appearance.theme !== nextTheme) {
        void updateAppearanceSettings({ theme: nextTheme });
      }
    };

    applySystemTheme();
    mediaQuery.addEventListener?.('change', applySystemTheme);
    return () => mediaQuery.removeEventListener?.('change', applySystemTheme);
  }, [
    settings.appearance.theme,
    settings.appearance.themeMode,
    settings.appearance.lightTheme,
    settings.appearance.darkTheme,
    updateAppearanceSettings,
  ]);

  useEffect(() => {
    const currentTheme = themes.find((theme) => theme.name === settings.appearance.theme);
    if (!currentTheme) return;

    const root = document.documentElement;
    root.style.setProperty('--tokyo-bg', currentTheme.colors.bg);
    root.style.setProperty('--tokyo-bg-dark', currentTheme.colors.bgDark);
    root.style.setProperty('--tokyo-bg-hl', currentTheme.colors.bgHl);
    root.style.setProperty('--tokyo-fg', currentTheme.colors.fg);
    root.style.setProperty('--tokyo-fg-dark', currentTheme.colors.fgDark);
    root.style.setProperty('--tokyo-comment', currentTheme.colors.fgDark);
    root.style.setProperty('--tokyo-selection', currentTheme.colors.bgHl);
    root.style.setProperty('--tokyo-blue', currentTheme.colors.accent);
    root.style.setProperty('--tokyo-on-accent', currentTheme.colors.onAccent);
    root.style.setProperty('--tokyo-red', currentTheme.colors.red);
    root.style.setProperty('--tokyo-green', currentTheme.colors.green);
    root.style.setProperty('--tokyo-yellow', currentTheme.colors.yellow);
    root.style.setProperty('--tokyo-magenta', currentTheme.colors.magenta);
    root.style.setProperty('--tokyo-cyan', currentTheme.colors.cyan);
    root.style.setProperty('--tokyo-orange', currentTheme.colors.orange);
    root.dataset.theme = currentTheme.name;
    const scheme = isLightTheme(currentTheme.name) ? 'light' : 'dark';
    root.dataset.appearance = scheme;
    root.style.colorScheme = scheme;
    // Match explicit themes, but release the native override in system mode;
    // otherwise WKWebView's prefers-color-scheme stops following macOS.
    if (platform === 'macos' && '__TAURI_INTERNALS__' in window) {
      const nativeTheme = settings.appearance.themeMode === 'system' ? null : scheme;
      void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().setTheme(nativeTheme)).catch(console.error);
    }
  }, [settings.appearance.theme, settings.appearance.themeMode, platform]);

  useCustomTheme(settings.appearance.theme);
}
