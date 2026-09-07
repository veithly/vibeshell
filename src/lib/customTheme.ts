import { create } from 'zustand';

export const CUSTOM_THEME_KEY = 'vibeshell.custom-theme.v1';
export const CUSTOM_THEME_EVENT = 'vibeshell:custom-theme-changed';
export const CUSTOM_CSS_LIMIT = 2 * 1024 * 1024;
const SAFE_KEY = 'vibeshell.custom-theme.disabled';
export interface CustomTheme { css: string; enabled: boolean }
export function parseCustomTheme(raw: string | null): CustomTheme {
  try {
    const value = JSON.parse(raw ?? 'null');
    if (value?.version === 1 && typeof value.css === 'string' && value.css.length <= CUSTOM_CSS_LIMIT) {
      return { css: value.css, enabled: value.enabled === true };
    }
  } catch { /* A broken import must not break application startup. */ }
  return { css: '', enabled: false };
}
function load(): CustomTheme {
  try {
    const theme = parseCustomTheme(localStorage.getItem(CUSTOM_THEME_KEY));
    return { ...theme, enabled: theme.enabled && localStorage.getItem(SAFE_KEY) !== '1' };
  } catch { return { css: '', enabled: false }; }
}
interface CustomThemeState extends CustomTheme {
  previewCss: string | null;
  save: (css: string, enabled: boolean) => void;
  preview: (css: string | null) => void;
  disable: () => void;
  reload: () => void;
}
function validate(css: string) {
  if (css.length > CUSTOM_CSS_LIMIT) throw new Error('CSS exceeds the 2 MiB theme limit');
}
export const useCustomThemeStore = create<CustomThemeState>((set, get) => ({
  ...load(), previewCss: null,
  save: (css, enabled) => {
    validate(css);
    localStorage.setItem(CUSTOM_THEME_KEY, JSON.stringify({ version: 1, css, enabled }));
    if (enabled) localStorage.removeItem(SAFE_KEY); else localStorage.setItem(SAFE_KEY, '1');
    set({ css, enabled, previewCss: null });
  },
  preview: previewCss => { if (previewCss !== null) validate(previewCss); set({ previewCss }); },
  disable: () => {
    // Recovery works even when persistent storage is full or inaccessible.
    set({ enabled: false, previewCss: null });
    document.getElementById('vibeshell-custom-css')?.remove();
    try { localStorage.setItem(SAFE_KEY, '1'); } catch { /* in-memory recovery remains effective */ }
    try { localStorage.setItem(CUSTOM_THEME_KEY, JSON.stringify({ version: 1, css: get().css, enabled: false })); } catch { /* keep the draft in memory */ }
  },
  reload: () => set({ ...load(), previewCss: null }),
}));

/** No HTML insertion or JavaScript execution. CSS is intentionally user-controlled. */
export function applyCustomCss(css: string | null): void {
  let style = document.getElementById('vibeshell-custom-css');
  if (css === null) { style?.remove(); }
  else {
    if (!style) { style = document.createElement('style'); style.id = 'vibeshell-custom-css'; document.head.appendChild(style); }
    style.textContent = css;
  }
  document.documentElement.dataset.customTheme = css === null ? 'off' : 'on';
  window.dispatchEvent(new Event(CUSTOM_THEME_EVENT));
}

export function customTerminalColors(): Record<string, string> {
  if (document.documentElement.dataset.customTheme !== 'on') return {};
  const computed = getComputedStyle(document.documentElement);
  const mapping: Record<string, string> = {
    background: 'bg', foreground: 'fg', cursor: 'blue', cursorAccent: 'bg', selectionBackground: 'selection',
    black: 'bg-hl', red: 'red', green: 'green', yellow: 'yellow', blue: 'blue', magenta: 'magenta', cyan: 'cyan', white: 'fg',
    brightBlack: 'fg-dark', brightRed: 'red', brightGreen: 'green', brightYellow: 'yellow', brightBlue: 'blue', brightMagenta: 'magenta', brightCyan: 'cyan', brightWhite: 'fg',
  };
  const colors: Record<string, string> = {};
  for (const [key, token] of Object.entries(mapping)) {
    const value = computed.getPropertyValue(`--tokyo-${token}`).trim();
    if (value && (!globalThis.CSS?.supports || CSS.supports('color', value))) colors[key] = value;
  }
  return colors;
}

export const CUSTOM_THEME_EXAMPLE = `/* VibeShell theme. CSS variables override the selected base theme. */
:root {
  --tokyo-blue: var(--tokyo-cyan) !important;
  --vibe-wallpaper-opacity: 0.12;
}
.session-tabbar { gap: 0.6rem; }
.session-tabbar [role="tab"] { border-radius: 8px; }
/* Add your own selectors, fonts, gradients, animations or ::before artwork.
   Recover at any time: Cmd/Ctrl+Shift+F12, or the native Workspace menu. */
`;

export function wallpaperCss(dataUrl: string): string {
  if (!/^data:image\/(png|jpeg|webp|gif|avif|bmp|svg\+xml);base64,[A-Za-z0-9+/=]+$/.test(dataUrl)) throw new Error('Unsupported image');
  return `\n/* Local image embedded in this shareable CSS, no network request. */\n.app-shell::after, [data-vibe-window="detached"]::after {\n  content: ""; position: fixed; inset: 0; pointer-events: none;\n  background: url("${dataUrl}") center / cover no-repeat;\n  opacity: var(--vibe-wallpaper-opacity, 0.12); z-index: 1;\n}\n`;
}
