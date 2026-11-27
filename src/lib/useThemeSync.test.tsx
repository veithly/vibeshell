import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useThemeSync } from './useThemeSync';
import { defaultSettings, normalizeTerminalBackgroundOpacity, normalizeWindowMaterialOpacity, useSettingsStore } from '../stores/settingsStore';
import { useRuntimeCapabilitiesStore, WEB_RUNTIME_CAPABILITIES } from '../stores/runtimeCapabilitiesStore';

vi.mock('./tauri', () => ({ invokeOrThrow: vi.fn(async () => null), safeInvoke: vi.fn(async () => ({ success: false })) }));
vi.mock('./useCustomTheme', () => ({ useCustomTheme: vi.fn() }));
const setNativeTheme = vi.hoisted(() => vi.fn(async () => {}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ setTheme: setNativeTheme }) }));

describe('glass theme synchronization', () => {
  beforeEach(() => {
    setNativeTheme.mockClear();
    useSettingsStore.setState({ settings: structuredClone(defaultSettings), initialized: true });
    useRuntimeCapabilitiesStore.setState({ capabilities: WEB_RUNTIME_CAPABILITIES, status: 'ready' });
    document.documentElement.removeAttribute('style');
  });
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

  it('defaults old settings to a 60% color mask and validates imported values', () => {
    for (const invalid of [undefined, null, NaN, Infinity, '0.9']) {
      expect(normalizeTerminalBackgroundOpacity(invalid)).toBe(0.6);
    }
    expect(normalizeTerminalBackgroundOpacity(-1)).toBe(0.5);
    expect(normalizeTerminalBackgroundOpacity(2)).toBe(1);
    expect(normalizeTerminalBackgroundOpacity(0.95)).toBe(0.95);
  });

  it('applies the window material slider independently of terminal and text opacity', async () => {
    expect(normalizeWindowMaterialOpacity(undefined)).toBe(0.12);
    expect(normalizeWindowMaterialOpacity(NaN)).toBe(0.12);
    expect(normalizeWindowMaterialOpacity(-1)).toBe(0);
    expect(normalizeWindowMaterialOpacity(2)).toBe(1);
    renderHook(() => useThemeSync());
    const root = document.documentElement;
    expect(root.style.getPropertyValue('--vibe-window-mask')).toBe('12%');
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({windowMaterialOpacity: 0.3}); });
    expect(root.style.getPropertyValue('--vibe-window-mask')).toBe('30%');
    expect(root.style.getPropertyValue('--vibe-terminal-mask')).toBe('60%');
    expect(root.style.opacity).toBe('');
  });

  it('switches day/night colors without reducing the opacity of text or controls', async () => {
    renderHook(() => useThemeSync());
    const root = document.documentElement;
    expect(root.dataset.appearance).toBe('light');
    expect(root.style.getPropertyValue('--tokyo-blue')).toBe('rgb(24, 24, 24)');
    expect(root.style.getPropertyValue('--tokyo-cyan')).toBe('rgb(24, 24, 24)');
    expect(root.style.getPropertyValue('--tokyo-on-accent')).toBe('rgb(245, 245, 245)');
    expect(root.style.getPropertyValue('--vibe-terminal-mask')).toBe('60%');
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({ theme: 'ink-black' }); });
    expect(root.dataset.appearance).toBe('dark');
    expect(root.style.getPropertyValue('--tokyo-blue')).toBe('rgb(245, 245, 245)');
    expect(root.style.getPropertyValue('--tokyo-cyan')).toBe('rgb(245, 245, 245)');
    expect(root.style.getPropertyValue('--tokyo-on-accent')).toBe('rgb(24, 24, 24)');
    expect(root.style.getPropertyValue('--vibe-syntax-accent')).not.toBe(root.style.getPropertyValue('--tokyo-blue'));
    expect(root.style.colorScheme).toBe('dark');
    expect(root.style.getPropertyValue('--vibe-terminal-mask')).toBe('60%');
    expect(root.style.opacity).toBe('');
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({ terminalBackgroundOpacity: 1 }); });
    expect(root.style.getPropertyValue('--vibe-terminal-mask')).toBe('100%');
  });

  it('follows system appearance changes and stops following in manual mode', async () => {
    const media = new EventTarget() as MediaQueryList;
    Object.defineProperty(media, 'matches', { value: false, writable: true });
    vi.stubGlobal('matchMedia', vi.fn(() => media));
    useSettingsStore.setState({ settings: { ...defaultSettings, appearance: { ...defaultSettings.appearance, themeMode: 'system' } } });
    renderHook(() => useThemeSync());
    await act(async () => {
      Object.defineProperty(media, 'matches', { value: true });
      media.dispatchEvent(new Event('change'));
    });
    expect(document.documentElement.dataset.theme).toBe(defaultSettings.appearance.darkTheme);
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({ themeMode: 'manual', theme: 'paper-white' }); });
    await act(async () => { media.dispatchEvent(new Event('change')); });
    expect(document.documentElement.dataset.theme).toBe('paper-white');
  });

  it('does not allow a dark preset to become the day shortcut (or vice versa)', async () => {
    await useSettingsStore.getState().updateAppearanceSettings({ lightTheme: 'ink-black', darkTheme: 'paper-white' });
    expect(useSettingsStore.getState().settings.appearance.lightTheme).toBe('paper-white');
    expect(useSettingsStore.getState().settings.appearance.darkTheme).toBe('ink-black');
  });

  it('releases the native appearance override when switching to system mode', async () => {
    vi.stubGlobal('__TAURI_INTERNALS__', {});
    const media = new EventTarget() as MediaQueryList;
    Object.defineProperty(media, 'matches', { value: false });
    vi.stubGlobal('matchMedia', vi.fn(() => media));
    useRuntimeCapabilitiesStore.setState({ capabilities: { ...WEB_RUNTIME_CAPABILITIES, platform: 'macos' } });
    renderHook(() => useThemeSync());
    await waitFor(() => expect(setNativeTheme).toHaveBeenLastCalledWith('light'));
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({ themeMode: 'system' }); });
    await waitFor(() => expect(setNativeTheme).toHaveBeenLastCalledWith(null));
    await act(async () => { await useSettingsStore.getState().updateAppearanceSettings({ themeMode: 'manual', theme: 'ink-black' }); });
    await waitFor(() => expect(setNativeTheme).toHaveBeenLastCalledWith('dark'));
  });
});
