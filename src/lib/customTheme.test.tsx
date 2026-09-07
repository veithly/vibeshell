import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { applyCustomCss, CUSTOM_THEME_KEY, parseCustomTheme, useCustomThemeStore, wallpaperCss } from './customTheme';
import { useCustomTheme } from './useCustomTheme';
function Harness() { useCustomTheme('paper-white'); return <button>Still usable</button>; }

describe('custom CSS themes', () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) });
    useCustomThemeStore.setState({ css: '', enabled: false, previewCss: null });
  });
  afterEach(() => { cleanup(); applyCustomCss(null); vi.unstubAllGlobals(); });
  it('does not accept malformed theme data', () => {
    for (const value of ['invalid', '{}', 'null', '{"version":1,"css":42}']) expect(parseCustomTheme(value).enabled).toBe(false);
  });
  it('inserts CSS as text instead of permitting HTML injection', () => {
    applyCustomCss('/* </style><script>window.pwned = true</script> */ body { opacity: .9 }');
    expect(document.getElementById('vibeshell-custom-css')?.textContent).toContain('</style>');
    expect(document.querySelector('script')).toBeNull();
  });
  it('previews without persisting and restores the saved theme', () => {
    const view = render(<Harness />);
    act(() => useCustomThemeStore.getState().save('body { opacity: .9 }', true));
    act(() => useCustomThemeStore.getState().preview('body { opacity: .8 }'));
    expect(document.getElementById('vibeshell-custom-css')?.textContent).toContain('.8');
    expect(localStorage.getItem(CUSTOM_THEME_KEY)).not.toContain('.8');
    act(() => useCustomThemeStore.getState().preview(null));
    expect(document.getElementById('vibeshell-custom-css')?.textContent).toContain('.9'); view.unmount();
  });
  it('emergency shortcut removes even CSS that hides every control', () => {
    render(<Harness />);
    act(() => useCustomThemeStore.getState().save('body { visibility: hidden !important }', true));
    fireEvent.keyDown(window, { key: 'F12', metaKey: true, shiftKey: true });
    expect(document.getElementById('vibeshell-custom-css')).toBeNull();
    expect(useCustomThemeStore.getState().enabled).toBe(false);
    useCustomThemeStore.getState().reload(); expect(useCustomThemeStore.getState().enabled).toBe(false);
  });
  it('can disable CSS even if persistent storage fails', () => {
    render(<Harness />); act(() => useCustomThemeStore.getState().preview('body { display: none }'));
    vi.stubGlobal('localStorage', { setItem: () => { throw new Error('quota exceeded'); } });
    act(() => useCustomThemeStore.getState().disable());
    expect(document.getElementById('vibeshell-custom-css')).toBeNull();
  });
  it('synchronizes saved CSS across document windows without feedback writes', () => {
    render(<Harness />);
    localStorage.setItem(CUSTOM_THEME_KEY, JSON.stringify({ version: 1, css: 'button { border-radius: 9px }', enabled: true }));
    fireEvent(window, new StorageEvent('storage', { key: CUSTOM_THEME_KEY }));
    expect(document.getElementById('vibeshell-custom-css')?.textContent).toContain('9px');
  });
  it('embeds images as a pointer-transparent background and rejects arbitrary CSS fragments', () => {
    const css = wallpaperCss('data:image/png;base64,AAAA'); expect(css).toContain('pointer-events: none');
    expect(css).toContain('data:image/png;base64,AAAA');
    expect(() => wallpaperCss('javascript:alert(1)')).toThrow();
    expect(() => wallpaperCss('data:image/png;base64,AAA\");body{display:none}')).toThrow();
  });
});
