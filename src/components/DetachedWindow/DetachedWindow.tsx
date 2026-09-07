import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ArrowDownLeft, Maximize2, Minus, X } from 'lucide-react';
import { emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useThemeSync } from '../../lib/useThemeSync';
import { Terminal } from '../Terminal';
import { PluginWorkspaceView } from '../PluginPanel/PluginWorkspaceView';
import { usePluginStore } from '../../stores/pluginStore';
import { useFileWorkspaceStore } from '../../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore } from '../../stores/pluginWorkspaceStore';
import { useSessionStore } from '../../stores/sessionStore';
import { captureTransfer, detachTargetKey, hydrateTransfer, readHandoff, DETACHED_READY_EVENT, WORKSPACE_QUITTING_EVENT, type DetachTarget } from '../../lib/detach';
import { captureWindowGeometry, restoreWindowGeometry, trackWindowGeometry } from '../../lib/windowGeometry';
import { dragNativeWindow, requestDock } from '../../lib/nativeDock';
import { setTextBufferLocked, waitForTextSave } from '../../lib/fileEditBuffer';
import { withDeadline } from '../../lib/deadline';

const FileWorkspace = lazy(() => import('../FileWorkspace').then((module) => ({ default: module.FileWorkspace })));
type Handoff = NonNullable<ReturnType<typeof readHandoff>>;
let bootstrap: Promise<Handoff> | null = null;
function bootstrapWindow(): Promise<Handoff> {
  if (bootstrap) return bootstrap;
  bootstrap = (async () => {
    const native = getCurrentWindow();
    const handoff = readHandoff(native.label);
    if (!handoff) throw new Error('Window content handoff is missing');
    hydrateTransfer(handoff.snapshot);
    if (handoff.snapshot.session) useSessionStore.getState().setActiveSession(handoff.snapshot.session.id);
    await new Promise<void>((resolve, reject) => {
      let stop: (() => void) | undefined;
      const timer = setTimeout(() => { stop?.(); reject(new Error('Main window did not commit the handoff')); }, 15000);
      void listen<{ label: string }>('vibeshell://detached-committed', ({ payload }) => {
        if (payload.label !== native.label) return;
        clearTimeout(timer); stop?.(); resolve();
      }).then((unlisten) => {
        stop = unlisten;
        return emitTo('main', DETACHED_READY_EVENT, { label: native.label });
      }).catch((error) => { clearTimeout(timer); stop?.(); reject(error); });
    });
    if (!handoff.drag) await restoreWindowGeometry(handoff.geometryKey).catch(console.error);
    await native.show();
    await native.setFocus();
    return handoff;
  })();
  return bootstrap;
}

export function DetachedWindow({ target }: { target: DetachTarget }) {
  useThemeSync();
  const { t } = useTranslation();
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transferring, setTransferring] = useState(false);
  const moving = useRef<AbortController | null>(null);
  const headerGesture = useRef<AbortController | null>(null);
  const quitting = useRef(false);
  const merging = useRef(false);
  const geometryKey = useRef(detachTargetKey(target));
  const files = useFileWorkspaceStore((state) => state.tabs);
  const plugins = usePluginWorkspaceStore((state) => state.tabs);
  const catalog = usePluginStore((state) => state.plugins);
  const file = target.kind === 'file' ? files.find((tab) => tab.path === target.path && tab.sessionId === target.sessionId) : null;
  const plugin = target.kind === 'plugin' ? plugins.find((tab) => tab.pluginId === target.pluginId && tab.sessionId === target.sessionId) : null;
  const title = target.kind === 'terminal' ? target.title : target.kind === 'file' ? target.name : `${target.pluginId} · ${target.serverName}`;

  const merge = useCallback(async (point?: { x: number; y: number }) => {
    if (merging.current || quitting.current) return;
    merging.current = true; setTransferring(true); setError(null);
    const fileId = target.kind === 'file' ? `${target.sessionId}\u0000${target.path}` : null;
    try {
      if (fileId) { setTextBufferLocked(fileId, true); await waitForTextSave(fileId); }
      await withDeadline(captureWindowGeometry(geometryKey.current), 1200, 'Window bounds save timed out').catch(console.error);
      const accepted = await requestDock(captureTransfer(target), point);
      if (accepted) await getCurrentWindow().destroy();
      else if (!point) setError(t('workspaceLayout.transferRejected'));
    } catch (reason) { setError(String(reason)); }
    finally { merging.current = false; setTransferring(false); if (fileId) setTextBufferLocked(fileId, false); }
  }, [target, t]);

  const drag = useCallback((tearOut = false) => {
    moving.current?.abort();
    const controller = new AbortController(); moving.current = controller;
    void dragNativeWindow(merge, tearOut, controller.signal).catch((reason) => setError(String(reason)));
  }, [merge]);

  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    void bootstrapWindow().then(async (handoff) => {
      if (disposed) return;
      geometryKey.current = handoff.geometryKey;
      setReady(true);
      void usePluginStore.getState().fetchPlugins();
      const installed = await Promise.all([
        trackWindowGeometry(handoff.geometryKey, false).catch((reason) => { console.warn('[Workspace] Geometry tracking unavailable:', reason); return () => {}; }),
        listen<{ quitting: boolean }>(WORKSPACE_QUITTING_EVENT, ({ payload }) => {
          quitting.current = payload?.quitting !== false;
          if (quitting.current) moving.current?.abort();
        }),
        getCurrentWindow().onCloseRequested((event) => {
          if (quitting.current) return;
          event.preventDefault();
          void merge();
        }),
      ]);
      if (disposed) installed.forEach((stop) => stop());
      else { stops.push(...installed); if (handoff.drag) drag(true); }
    }).catch(async (reason) => {
      if (disposed) return;
      setError(String(reason));
      await getCurrentWindow().show().catch(console.error);
    });
    return () => { disposed = true; moving.current?.abort(); headerGesture.current?.abort(); stops.forEach((stop) => stop()); };
  }, [drag, merge]);

  const expand = useCallback(async () => {
    try {
      const native = getCurrentWindow();
      if (/Mac/.test(navigator.platform)) await native.setFullscreen(!(await native.isFullscreen()));
      else await native.toggleMaximize();
    } catch (reason) { setError(String(reason)); }
  }, []);

  return (
    <div data-vibe-window="detached" className="flex h-screen min-h-0 flex-col overflow-hidden bg-tokyo-bg text-tokyo-fg">
      <header className="flex h-9 shrink-0 select-none items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark pl-3"
        onMouseDown={(event) => {
          if (!ready || event.button !== 0 || event.detail > 1 || (event.target as Element).closest('button')) return;
          event.preventDefault();
          headerGesture.current?.abort();
          const gesture = new AbortController(); headerGesture.current = gesture;
          const startX = event.clientX; const startY = event.clientY;
          document.addEventListener('mousemove', (move) => {
            if (!(move.buttons & 1)) { gesture.abort(); return; }
            if (Math.hypot(move.clientX - startX, move.clientY - startY) < 5) return;
            gesture.abort(); drag();
          }, { signal: gesture.signal });
          document.addEventListener('mouseup', () => gesture.abort(), { once: true, signal: gesture.signal });
          window.addEventListener('blur', () => gesture.abort(), { once: true, signal: gesture.signal });
        }} onDoubleClick={(event) => {
          if (!(event.target as Element).closest('button')) void expand();
        }}>
        <span className="min-w-0 flex-1 truncate text-xs" title={title}>{file?.dirty ? '● ' : ''}{title}</span>
        <button type="button" className="icon-button" aria-label={t('workspaceLayout.mergeBack')} title={t('workspaceLayout.mergeBack')} disabled={!ready || transferring} onClick={() => void merge()}><ArrowDownLeft className="h-4 w-4" /></button>
        <button type="button" className="icon-button" aria-label={t('workspaceLayout.minimize')} onClick={() => void getCurrentWindow().minimize()}><Minus className="h-3.5 w-3.5" /></button>
        <button type="button" className="icon-button" aria-label={t('workspaceLayout.maximize')} onClick={() => void expand()}><Maximize2 className="h-3.5 w-3.5" /></button>
        <button type="button" className="icon-button" aria-label={t('workspaceLayout.closeAndReturn')} title={t('workspaceLayout.closeAndReturn')} disabled={transferring} onClick={() => ready ? void merge() : void getCurrentWindow().destroy()}><X className="h-4 w-4" /></button>
      </header>
      {error && <div role="alert" className="border-b border-tokyo-bg-hl p-3 text-xs text-tokyo-red">{error}</div>}
      <main className="min-h-0 flex-1">
        {!ready ? <div role="status" className="p-4 text-sm text-tokyo-comment">{t('common.loading')}</div>
          : target.kind === 'terminal' ? <Terminal sessionId={target.sessionId} onData={() => {}} />
          : file ? <Suspense fallback={<div className="p-4">{t('common.loading')}</div>}><FileWorkspace tab={file} isActive /></Suspense>
          : plugin ? <PluginWorkspaceView tab={plugin} plugin={catalog.find((item) => item.manifest.id === plugin.pluginId)} onClose={() => void merge()} /> : null}
      </main>
    </div>
  );
}
