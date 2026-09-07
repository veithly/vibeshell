import { create } from 'zustand';
import i18n from 'i18next';
import { useNotificationStore } from '../stores/notificationStore';
import { useSessionStore, type Session } from '../stores/sessionStore';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore, type PluginPanelState, type PluginWorkspaceTab } from '../stores/pluginWorkspaceStore';
import { filePaneId, pluginPaneId, sessionPaneId } from './paneIds';
import { readTextBuffer, writeTextBuffer, setTextBufferLocked, waitForTextSave, type TextEditBuffer } from './fileEditBuffer';

export type DetachTarget =
  | { kind: 'terminal'; sessionId: string; title: string }
  | { kind: 'plugin'; pluginId: string; sessionId: string; serverName: string; sessionType: 'ssh' | 'local' }
  | { kind: 'file'; source?: 'local'; sessionId: string; path: string; name: string; size: number };
export const DETACHED_READY_EVENT = 'vibeshell://detached-ready';
export const DETACHED_CLOSED_EVENT = 'vibeshell://detached-closed';
export const WORKSPACE_QUITTING_EVENT = 'vibeshell://workspace-quitting';
export const SESSION_TAB_DND_MIME = 'application/x-vibeshell-session-tab';
export const PLUGIN_TAB_DND_MIME = 'application/x-vibeshell-plugin-tab';
export const FILE_TAB_DND_MIME = 'application/x-vibeshell-file-tab';
const HANDOFF_PREFIX = 'vibeshell.window-handoff.';
const LAYOUT_KEY = 'vibeshell.detached-layout.v2';

export interface TransferSnapshot {
  target: DetachTarget;
  session?: Session;
  file?: FileWorkspaceTab;
  buffer?: TextEditBuffer | null;
  plugin?: PluginWorkspaceTab;
  panel?: PluginPanelState;
}
export interface DetachedLayoutEntry { target: DetachTarget; geometryKey: string }
export const useDetachedOwnership = create<{ owners: Record<string, string> }>(() => ({ owners: {} }));
const pending = new Map<string, Promise<string | null>>();

export function canCloseWorkspaceSession(sessionId: string): boolean {
  const owners = useDetachedOwnership.getState().owners;
  const inUse = readDetachedLayout().some((entry) => entry.target.sessionId === sessionId && owners[detachTargetKey(entry.target)]);
  if (inUse) useNotificationStore.getState().warning(i18n.t('workspaceLayout.mergeBack'), i18n.t('workspaceLayout.returnBeforeClose'));
  return !inUse;
}

export function detachTargetKey(target: DetachTarget): string {
  if (target.kind === 'terminal') return sessionPaneId(target.sessionId);
  if (target.kind === 'plugin') return pluginPaneId(`${target.sessionId}::${target.pluginId}`);
  return filePaneId(`${target.sessionId}\u0000${target.path}`);
}
export function detachQueryString(target: DetachTarget): string {
  const params = new URLSearchParams({ detach: target.kind, session: target.sessionId });
  if (target.kind === 'terminal') params.set('title', target.title);
  if (target.kind === 'plugin') { params.set('plugin', target.pluginId); params.set('server', target.serverName); params.set('type', target.sessionType); }
  if (target.kind === 'file') { params.set('path', target.path); params.set('name', target.name); params.set('size', String(target.size)); if (target.source === 'local') params.set('source', 'local'); }
  return params.toString();
}
export function parseDetachTarget(search: string): DetachTarget | null {
  const p = new URLSearchParams(search);
  const sessionId = p.get('session');
  if (!sessionId || sessionId.length > 512) return null;
  if (p.get('detach') === 'terminal') return { kind: 'terminal', sessionId, title: p.get('title') ?? 'Terminal' };
  if (p.get('detach') === 'plugin' && p.get('plugin')) return { kind: 'plugin', sessionId, pluginId: p.get('plugin')!, serverName: p.get('server') ?? '', sessionType: p.get('type') === 'local' ? 'local' : 'ssh' };
  if (p.get('detach') === 'file' && p.get('path') && p.get('name')) return { kind: 'file', ...(p.get('source') === 'local' ? { source: 'local' as const } : {}), sessionId, path: p.get('path')!, name: p.get('name')!, size: Math.max(0, Number(p.get('size')) || 0) };
  return null;
}
export function isDetachedWindowContext(): boolean { return typeof window !== 'undefined' && parseDetachTarget(window.location.search) !== null; }

export function captureTransfer(target: DetachTarget): TransferSnapshot {
  const session = useSessionStore.getState().sessions.find((candidate) => candidate.id === target.sessionId);
  if (!session && !(target.kind === 'file' && target.source === 'local')) throw new Error('Session no longer exists');
  const snapshot: TransferSnapshot = { target, session };
  if (target.kind === 'file') {
    snapshot.file = useFileWorkspaceStore.getState().tabs.find((tab) => tab.sessionId === target.sessionId && tab.path === target.path);
    if (!snapshot.file) throw new Error('File tab no longer exists');
    snapshot.buffer = readTextBuffer(snapshot.file.id);
  }
  if (target.kind === 'plugin') {
    const id = `${target.sessionId}::${target.pluginId}`;
    snapshot.plugin = usePluginWorkspaceStore.getState().tabs.find((tab) => tab.id === id);
    snapshot.panel = usePluginWorkspaceStore.getState().panels[id];
  }
  return snapshot;
}
export function hydrateTransfer(snapshot: TransferSnapshot): void {
  const local = snapshot.target.kind === 'file' && snapshot.target.source === 'local';
  const session = snapshot.session;
  if (!local && (!session || session.id !== snapshot.target.sessionId)) throw new Error('Invalid window handoff');
  if (snapshot.target.kind === 'file' && local && (!snapshot.file || snapshot.file.source !== 'local' || snapshot.file.path !== snapshot.target.path
    || snapshot.file.sessionId !== snapshot.target.sessionId)) throw new Error('Invalid local file handoff');
  if (session) useSessionStore.setState((state) => ({ sessions: state.sessions.some((s) => s.id === session.id)
    ? state.sessions : [...state.sessions, session] }));
  if (snapshot.file) {
    if (snapshot.buffer) writeTextBuffer(snapshot.file.id, snapshot.buffer);
    const tab = { ...snapshot.file, dirty: snapshot.buffer ? snapshot.buffer.text !== snapshot.buffer.saved : snapshot.file.dirty };
    useFileWorkspaceStore.setState((state) => ({ tabs: state.tabs.some((t) => t.id === tab.id)
      ? state.tabs.map((t) => t.id === tab.id ? tab : t) : [...state.tabs, tab] }));
  }
  if (snapshot.plugin) {
    const tab = snapshot.plugin;
    usePluginWorkspaceStore.setState((state) => ({
      tabs: state.tabs.some((t) => t.id === tab.id) ? state.tabs : [...state.tabs, tab],
      panels: snapshot.panel ? { ...state.panels, [tab.id]: snapshot.panel } : state.panels,
    }));
  }
}
export function readDetachedLayout(): DetachedLayoutEntry[] {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? '[]');
    if (!Array.isArray(raw)) return [];
    return raw.slice(0, 32).filter((entry) => entry?.target && typeof entry.geometryKey === 'string'
      && parseDetachTarget(detachQueryString(entry.target)) !== null);
  } catch { return []; }
}
export function saveDetachedLayout(entries: DetachedLayoutEntry[]): void { localStorage.setItem(LAYOUT_KEY, JSON.stringify(entries)); }
export function addDetachedToLayout(target: DetachTarget, geometryKey = detachTargetKey(target)): void {
  const key = detachTargetKey(target);
  saveDetachedLayout([...readDetachedLayout().filter((entry) => detachTargetKey(entry.target) !== key), { target, geometryKey }]);
}
export function removeDetachedFromLayout(target: DetachTarget): void {
  const key = detachTargetKey(target);
  saveDetachedLayout(readDetachedLayout().filter((entry) => detachTargetKey(entry.target) !== key));
}
export function releaseDetached(target: DetachTarget): void {
  const key = detachTargetKey(target);
  useDetachedOwnership.setState((state) => { const owners = { ...state.owners }; delete owners[key]; return { owners }; });
  removeDetachedFromLayout(target);
}
export function readHandoff(label: string): { snapshot: TransferSnapshot; drag: boolean; geometryKey: string } | null {
  try { const raw = localStorage.getItem(HANDOFF_PREFIX + label); return raw ? JSON.parse(raw) : null; }
  catch { return null; }
}
export function clearHandoff(label: string): void { localStorage.removeItem(HANDOFF_PREFIX + label); }

export function openDetachedWindow(
  target: DetachTarget,
  options?: { x?: number; y?: number; width?: number; height?: number; drag?: boolean; geometryKey?: string }
): Promise<string | null> {
  const key = detachTargetKey(target);
  if (pending.has(key)) return pending.get(key)!;
  const operation = (async () => {
    if (!('__TAURI_INTERNALS__' in window)) return null;
    const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    const { listen, emitTo } = await import('@tauri-apps/api/event');
    const existingLabel = useDetachedOwnership.getState().owners[key];
    if (existingLabel) {
      const existing = await WebviewWindow.getByLabel(existingLabel);
      if (existing) { await existing.unminimize(); await existing.setFocus(); return existingLabel; }
      releaseDetached(target);
    }
    const label = `detach-${crypto.randomUUID()}`;
    const geometryKey = options?.geometryKey ?? key;
    let stopReady: (() => void) | undefined;
    let stopError: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let finished = false;
    const fileId = target.kind === 'file' ? `${target.sessionId}\u0000${target.path}` : null;
    try {
      if (fileId) { setTextBufferLocked(fileId, true); await waitForTextSave(fileId); }
      const snapshot = captureTransfer(target);
      localStorage.setItem(HANDOFF_PREFIX + label, JSON.stringify({ snapshot, drag: options?.drag ?? false, geometryKey }));
      const ready = new Promise<void>((resolve, reject) => {
        timer = setTimeout(() => reject(new Error('Detached window did not acknowledge its content')), 15000);
        void listen<{ label: string }>(DETACHED_READY_EVENT, ({ payload }) => {
          if (payload.label === label) resolve();
        }).then((stop) => {
          if (finished) { stop(); return; }
          stopReady = stop;
          const nativeWindow = new WebviewWindow(label, {
            url: `index.html?${detachQueryString(target)}`,
            title: target.kind === 'terminal' ? target.title : target.kind === 'file' ? target.name : target.pluginId,
            width: options?.width ?? 980, height: options?.height ?? 660,
            minWidth: 480, minHeight: 320, decorations: false, dragDropEnabled: false, visible: false,
          });
          void nativeWindow.once('tauri://error', ({ payload: error }) => reject(new Error(String(error))))
            .then((unlisten) => { if (finished) unlisten(); else stopError = unlisten; }).catch(reject);
        }).catch(reject);
      });
      await ready;
      addDetachedToLayout(target, geometryKey);
      useDetachedOwnership.setState((state) => ({ owners: { ...state.owners, [key]: label } }));
      await emitTo(label, 'vibeshell://detached-committed', { label });
      return label;
    } catch (error) {
      console.error('[Workspace] Could not detach tab:', error);
      useNotificationStore.getState().error(i18n.t('workspaceLayout.detach'), i18n.t('workspaceLayout.detachFailed'));
      if (useDetachedOwnership.getState().owners[key] === label) releaseDetached(target);
      const failed = await WebviewWindow.getByLabel(label);
      await failed?.destroy().catch(console.error);
      return null;
    } finally {
      finished = true;
      clearTimeout(timer); stopReady?.(); stopError?.();
      try { clearHandoff(label); } catch (error) { console.error('[Workspace] Handoff cleanup failed:', error); }
      if (fileId) setTextBufferLocked(fileId, false);
    }
  })().finally(() => pending.delete(key));
  pending.set(key, operation);
  return operation;
}

/** Called only after saved session identifiers have been resolved. */
export async function restoreDetachedWindows(entries = readDetachedLayout()): Promise<void> {
  if (isDetachedWindowContext()) return;
  for (const entry of entries) {
    if (useSessionStore.getState().sessions.some((s) => s.id === entry.target.sessionId)) {
      await openDetachedWindow(entry.target, { geometryKey: entry.geometryKey });
    }
  }
}
