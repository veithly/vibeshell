import { useEffect } from 'react';
import { applyCustomCss, CUSTOM_THEME_KEY, useCustomThemeStore } from './customTheme';

export function useCustomTheme(baseTheme: string): void {
  const { css, enabled, previewCss } = useCustomThemeStore();
  useEffect(() => {
    const safe = new URLSearchParams(location.search).has('safe-theme');
    applyCustomCss(safe ? null : previewCss ?? (enabled ? css : null));
    return () => applyCustomCss(null);
  }, [css, enabled, previewCss, baseTheme]);
  useEffect(() => {
    const recover = () => useCustomThemeStore.getState().disable();
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key === 'F12') {
        event.preventDefault(); event.stopImmediatePropagation(); recover();
      }
    };
    const onStorage = (event: StorageEvent) => {
      if (event.key === CUSTOM_THEME_KEY || event.key === 'vibeshell.custom-theme.disabled' || event.key === null) useCustomThemeStore.getState().reload();
    };
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('storage', onStorage);
    let disposed = false; let stop: (() => void) | undefined;
    if ('__TAURI_INTERNALS__' in window) {
      void import('@tauri-apps/api/event').then(({ listen }) => listen('vibeshell://disable-custom-css', recover))
        .then(unlisten => { if (disposed) unlisten(); else stop = unlisten; }).catch(console.error);
    }
    return () => { disposed = true; stop?.(); window.removeEventListener('keydown', onKey, true); window.removeEventListener('storage', onStorage); };
  }, []);
}
