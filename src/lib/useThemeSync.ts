import { useEffect } from 'react';
import { useSettingsStore, themes } from '../stores/settingsStore';

/**
 * Applies the selected theme to the document root and keeps a `system` theme
 * mode in sync with the OS. Shared by the main window and detached windows so
 * torn-out tabs look identical to the main app.
 */
export function useThemeSync(): void {
  const settings = useSettingsStore((state) => state.settings);
  const initializeSettings = useSettingsStore((state) => state.initializeSettings);
  const updateAppearanceSettings = useSettingsStore((state) => state.updateAppearanceSettings);

  useEffect(() => {
    void initializeSettings();
  }, [initializeSettings]);

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
    root.style.colorScheme = currentTheme.name === 'paper-white' || currentTheme.name === 'warm-ivory'
      ? 'light'
      : 'dark';
  }, [settings.appearance.theme]);
}
