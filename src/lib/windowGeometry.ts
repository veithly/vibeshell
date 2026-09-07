import { availableMonitors, getCurrentWindow, type Window as NativeWindow } from '@tauri-apps/api/window';
import { physicalPosition, physicalSize } from './physicalPixels';
import { fitWindowRect, type Rect } from './docking';
import { useRuntimeCapabilitiesStore } from '../stores/runtimeCapabilitiesStore';

const capturing = new Map<string, Promise<void>>();

export interface WindowGeometry extends Rect { maximized?: boolean; fullscreen?: boolean }
const PREFIX = 'vibeshell.window-geometry.v2.';
export function readWindowGeometry(key: string): WindowGeometry | null {
  try {
    const raw = localStorage.getItem(PREFIX + encodeURIComponent(key));
    const value = raw ? JSON.parse(raw) as WindowGeometry : null;
    return value && [value.x, value.y, value.width, value.height].every(Number.isFinite)
      && value.width > 0 && value.height > 0 ? value : null;
  } catch { return null; }
}

export async function restoreWindowGeometry(key: string, nativeWindow: NativeWindow = getCurrentWindow()): Promise<void> {
  const stored = readWindowGeometry(key);
  if (!stored) return;
  const monitors = await availableMonitors();
  const areas = monitors.map((m) => ({ x: m.workArea.position.x, y: m.workArea.position.y, width: m.workArea.size.width, height: m.workArea.size.height }));
  const rect = fitWindowRect(stored, areas);
  await nativeWindow.setPosition(physicalPosition(rect.x, rect.y));
  await nativeWindow.setSize(physicalSize(rect.width, rect.height));
  if (stored.maximized && !/Mac/.test(navigator.platform)) await nativeWindow.maximize();
  if (stored.fullscreen) await nativeWindow.setFullscreen(true);
}

export function captureWindowGeometry(key: string, nativeWindow: NativeWindow = getCurrentWindow()): Promise<void> {
  const inflight = capturing.get(key);
  if (inflight) return inflight;
  const capture = (async () => {
    const platform = useRuntimeCapabilitiesStore.getState().capabilities.platform;
    const macos = platform === 'macos' || /Mac/.test(navigator.platform);
    const [position, size, maximized, fullscreen] = await Promise.all([
      nativeWindow.outerPosition(), nativeWindow.innerSize(),
      macos ? Promise.resolve(false) : nativeWindow.isMaximized(), nativeWindow.isFullscreen(),
    ]);
    const previous = readWindowGeometry(key);
    const normal = (maximized || fullscreen) && previous ? previous : { x: position.x, y: position.y, width: size.width, height: size.height };
    localStorage.setItem(PREFIX + encodeURIComponent(key), JSON.stringify({ ...normal, maximized, fullscreen }));
  })().finally(() => capturing.delete(key));
  capturing.set(key, capture);
  return capture;
}

/** Debounced move/resize saves plus close flush; normal bounds survive maximization. */
export async function trackWindowGeometry(key: string, restore = true): Promise<() => void> {
  if (!('__TAURI_INTERNALS__' in window)) return () => {};
  const nativeWindow = getCurrentWindow();
  if (restore) await restoreWindowGeometry(key, nativeWindow);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let saving = false;
  const save = () => {
    if (disposed) return;
    clearTimeout(timer);
    timer = setTimeout(() => {
      if (disposed || saving) return;
      saving = true;
      void captureWindowGeometry(key, nativeWindow).catch(console.error).finally(() => { saving = false; });
    }, 200);
  };
  const stops = await Promise.all([
    nativeWindow.onMoved(save), nativeWindow.onResized(save), nativeWindow.onScaleChanged(save),
  ]);
  const flush = () => { void captureWindowGeometry(key, nativeWindow).catch(console.error); };
  window.addEventListener('pagehide', flush);
  save();
  return () => { disposed = true; clearTimeout(timer); stops.forEach((stop) => stop()); window.removeEventListener('pagehide', flush); };
}
