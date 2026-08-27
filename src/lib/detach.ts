import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/**
 * Detached (torn-out) windows. A tab dragged out of the main window opens in
 * its own OS window running the same frontend with a `detach` query payload;
 * closing or merging it back notifies the main window via a Tauri event.
 *
 * The set of open detached windows (with geometry) is persisted so the next
 * app launch restores the same workspace.
 */

export type DetachTarget =
  | { kind: 'terminal'; sessionId: string; title: string }
  | {
      kind: 'plugin';
      pluginId: string;
      sessionId: string;
      serverName: string;
      sessionType: 'ssh' | 'local';
    };

export const DETACHED_CLOSED_EVENT = 'vibeshell://detached-closed';

/** Drag payload MIME types shared by the tab strip and pane drop zones. */
export const SESSION_TAB_DND_MIME = 'application/x-vibeshell-session-tab';
export const PLUGIN_TAB_DND_MIME = 'application/x-vibeshell-plugin-tab';
export const FILE_TAB_DND_MIME = 'application/x-vibeshell-file-tab';

export const TAB_DND_MIMES = [SESSION_TAB_DND_MIME, PLUGIN_TAB_DND_MIME, FILE_TAB_DND_MIME];

export function detachQueryString(target: DetachTarget): string {
  const params = new URLSearchParams({ detach: target.kind });
  if (target.kind === 'terminal') {
    params.set('session', target.sessionId);
    params.set('title', target.title);
  } else {
    params.set('plugin', target.pluginId);
    params.set('session', target.sessionId);
    params.set('server', target.serverName);
    params.set('type', target.sessionType);
  }
  return params.toString();
}

export function detachTargetKey(target: DetachTarget): string {
  return target.kind === 'terminal'
    ? `terminal:${target.sessionId}`
    : `plugin:${target.sessionId}::${target.pluginId}`;
}

export function parseDetachTarget(search: string): DetachTarget | null {
  const params = new URLSearchParams(search);
  const kind = params.get('detach');
  const session = params.get('session');
  if (kind === 'terminal' && session) {
    return { kind: 'terminal', sessionId: session, title: params.get('title') ?? '' };
  }
  if (kind === 'plugin' && session && params.get('plugin')) {
    return {
      kind: 'plugin',
      pluginId: params.get('plugin') as string,
      sessionId: session,
      serverName: params.get('server') ?? '',
      sessionType: params.get('type') === 'local' ? 'local' : 'ssh',
    };
  }
  return null;
}

export function isDetachedWindowContext(): boolean {
  return parseDetachTarget(window.location.search) !== null;
}

// ---------------------------------------------------------------------------
// Layout persistence
// ---------------------------------------------------------------------------

interface DetachedLayoutEntry {
  target: DetachTarget;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
}

const LAYOUT_KEY = 'vibeshell.detached-layout.v1';

function readLayout(): DetachedLayoutEntry[] {
  try {
    const raw = window.localStorage.getItem(LAYOUT_KEY);
    if (!raw) return [];
    const entries = JSON.parse(raw) as DetachedLayoutEntry[];
    return Array.isArray(entries)
      ? entries.filter((entry) => entry?.target?.kind)
      : [];
  } catch {
    return [];
  }
}

function writeLayout(entries: DetachedLayoutEntry[]) {
  try {
    window.localStorage.setItem(LAYOUT_KEY, JSON.stringify(entries));
  } catch (error) {
    console.error('[Detach] Failed to persist window layout:', error);
  }
}

export function addDetachedToLayout(target: DetachTarget) {
  const entries = readLayout();
  const key = detachTargetKey(target);
  if (!entries.some((entry) => detachTargetKey(entry.target) === key)) {
    entries.push({ target });
    writeLayout(entries);
  }
}

export function updateDetachedLayoutGeometry(
  target: DetachTarget,
  geometry: { x?: number; y?: number; width?: number; height?: number }
) {
  const entries = readLayout();
  const key = detachTargetKey(target);
  const entry = entries.find((candidate) => detachTargetKey(candidate.target) === key);
  if (entry) {
    Object.assign(entry, geometry);
    writeLayout(entries);
  }
}

export function removeDetachedFromLayout(target: DetachTarget) {
  const key = detachTargetKey(target);
  writeLayout(readLayout().filter((entry) => detachTargetKey(entry.target) !== key));
}

let detachCounter = 0;

export async function openDetachedWindow(
  target: DetachTarget,
  geometry?: { x?: number; y?: number; width?: number; height?: number }
): Promise<string | null> {
  const label = `detach-${Date.now()}-${detachCounter++}`;
  const title = target.kind === 'terminal'
    ? target.title || target.sessionId
    : `${target.pluginId} · ${target.serverName}`;
  const webview = new WebviewWindow(label, {
    url: `index.html?${detachQueryString(target)}`,
    title,
    width: geometry?.width ?? 980,
    height: geometry?.height ?? 660,
    x: geometry?.x,
    y: geometry?.y,
    minWidth: 480,
    minHeight: 320,
    center: geometry?.x === undefined && geometry?.y === undefined,
  });
  webview.once('tauri://error', (event) => {
    console.error('[Detach] Failed to open detached window:', event.payload);
  });
  // Resolve once the window exists so callers can hand it to the native drag
  // loop (`startDragging`) or focus it.
  return new Promise((resolve) => {
    webview.once('tauri://created', () => resolve(label));
    webview.once('tauri://error', () => resolve(null));
  });
}

let restoredOnce = false;

/**
 * Re-open the detached windows from the previous session. Runs only in the
 * main window, only once per page load (StrictMode-safe).
 */
export async function restoreDetachedWindows(): Promise<void> {
  if (restoredOnce || isDetachedWindowContext()) return;
  restoredOnce = true;
  for (const entry of readLayout()) {
    await openDetachedWindow(entry.target, entry);
  }
}
