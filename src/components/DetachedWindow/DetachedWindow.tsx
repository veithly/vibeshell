import { lazy, Suspense, useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { CornerDownLeft } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useThemeSync } from '../../lib/useThemeSync';
import {
  DETACHED_CLOSED_EVENT,
  addDetachedToLayout,
  removeDetachedFromLayout,
  updateDetachedLayoutGeometry,
  type DetachTarget,
} from '../../lib/detach';
import { usePluginStore } from '../../stores/pluginStore';
import type { PluginWorkspaceTab } from '../../stores/pluginWorkspaceStore';

const Terminal = lazy(() => import('../Terminal').then((mod) => ({ default: mod.Terminal })));
const PluginWorkspaceView = lazy(() => import('../PluginPanel/PluginWorkspaceView').then((mod) => ({ default: mod.PluginWorkspaceView })));

/**
 * Content of a torn-out tab window. The webview is created by the main window
 * with a `detach` query payload; this component re-hosts the terminal or
 * plugin panel and offers a "merge back" affordance, since HTML5 drag cannot
 * cross native window boundaries in the other direction.
 */
export function DetachedWindow({ target }: { target: DetachTarget }) {
  const { t } = useTranslation();
  useThemeSync();
  const plugins = usePluginStore((state) => state.plugins);
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);

  useEffect(() => {
    void fetchPlugins();
  }, [fetchPlugins]);

  const title = target.kind === 'terminal'
    ? (target.title || target.sessionId)
    : `${target.pluginId} · ${target.serverName}`;

  const plugin = target.kind === 'plugin'
    ? plugins.find((candidate) => candidate.manifest.id === target.pluginId)
    : undefined;

  const detachedTab = useMemo<PluginWorkspaceTab | null>(() => (
    target.kind === 'plugin'
      ? {
          id: `${target.sessionId}::${target.pluginId}`,
          pluginId: target.pluginId,
          sessionId: target.sessionId,
          sessionType: target.sessionType,
          serverName: target.serverName,
        }
      : null
  ), [target]);

  useEffect(() => {
    // Register in the persisted layout so the next app launch restores this
    // window. localStorage is shared across the app's webviews.
    addDetachedToLayout(target);
  }, [target]);

  useEffect(() => {
    const currentWindow = getCurrentWindow();
    let announced = false;

    const announce = async () => {
      if (announced) return;
      announced = true;
      // Persist the final geometry before the window goes away so the next
      // launch restores this window where the user left it.
      try {
        const [position, size] = await Promise.all([
          currentWindow.outerPosition(),
          currentWindow.outerSize(),
        ]);
        updateDetachedLayoutGeometry(target, {
          x: position.x,
          y: position.y,
          width: size.width,
          height: size.height,
        });
      } catch (error) {
        console.error('[Detach] Failed to persist window geometry:', error);
      }
      const payload = target.kind === 'terminal'
        ? { kind: 'terminal', sessionId: target.sessionId }
        : { kind: 'plugin', pluginId: target.pluginId, sessionId: target.sessionId };
      try {
        await emit(DETACHED_CLOSED_EVENT, payload);
      } catch (error) {
        console.error('[Detach] Failed to announce window close:', error);
      }
    };

    // Intercept the native close button so the main window learns the tab
    // should become active again; then allow the close to proceed.
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void currentWindow
      .listen('tauri://close-requested', async () => {
        await announce();
        window.setTimeout(() => {
          if (!disposed) void currentWindow.close();
        }, 120);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [target]);

  const mergeBack = async () => {
    const payload = target.kind === 'terminal'
      ? { kind: 'terminal', sessionId: target.sessionId }
      : { kind: 'plugin', pluginId: target.pluginId, sessionId: target.sessionId };
    removeDetachedFromLayout(target);
    try {
      await emit(DETACHED_CLOSED_EVENT, payload);
    } finally {
      await getCurrentWindow().close();
    }
  };

  return (
    <div className="app-shell flex h-screen flex-col bg-tokyo-bg">
      <header className="flex h-9 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3" data-tauri-drag-region>
        <span className="min-w-0 flex-1 truncate text-xs font-semibold text-tokyo-fg" data-tauri-drag-region>
          {title}
        </span>
        <button
          className={cn(
            'flex h-6 items-center gap-1.5 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2',
            'text-[11px] text-tokyo-comment transition-colors hover:border-tokyo-cyan hover:text-tokyo-fg'
          )}
          onClick={() => void mergeBack()}
          title={t('plugins.detach.mergeBack')}
        >
          <CornerDownLeft className="h-3 w-3" />
          {t('plugins.detach.mergeBack')}
        </button>
      </header>

      <main className="min-h-0 flex-1">
        {target.kind === 'terminal' ? (
          <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
            <Terminal sessionId={target.sessionId} onData={() => {}} />
          </Suspense>
        ) : detachedTab ? (
          <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
            <PluginWorkspaceView tab={detachedTab} plugin={plugin} />
          </Suspense>
        ) : null}
      </main>
    </div>
  );
}
