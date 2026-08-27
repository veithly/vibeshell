import { lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import { Columns2, Rows2, X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { localizedPluginName } from '../../plugins/pluginUtils';
import type { PluginRecord } from '../../plugins/types';
import type { PluginWorkspaceTab } from '../../stores/pluginWorkspaceStore';
import { PluginIcon } from '../PluginIcon';

const PluginPanel = lazy(() => import('./PluginPanel').then((mod) => ({ default: mod.PluginPanel })));

interface PluginWorkspaceViewProps {
  tab: PluginWorkspaceTab;
  plugin: PluginRecord | undefined;
  onClose?: () => void;
  /** When omitted (detached windows), the split controls are hidden. */
  onSplit?: (tabId: string, direction: 'row' | 'column') => void;
}

/**
 * Full-area content for an active plugin tab: a header with split actions and
 * the shared PluginPanel runner. Once split, the same panel renders inside a
 * mosaic pane instead.
 */
export function PluginWorkspaceView({ tab, plugin, onClose, onSplit }: PluginWorkspaceViewProps) {
  const { t } = useTranslation();
  const pluginName = plugin
    ? localizedPluginName(t, plugin)
    : tab.pluginId;

  return (
    <div className="flex h-full min-h-0 flex-col bg-tokyo-bg">
      <header className="flex h-10 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3">
        <PluginIcon name={plugin?.manifest.icon ?? 'plug'} className="h-4 w-4 flex-shrink-0" />
        <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-tokyo-fg">
          {pluginName}
          <span className="ml-2 font-normal text-tokyo-comment">{tab.serverName}</span>
        </h2>
        <div className="flex flex-shrink-0 items-center gap-1">
          {onSplit && (
            <>
              <button
                className={cn(
                  'flex h-7 w-7 items-center justify-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg',
                  'text-tokyo-comment transition-colors hover:border-tokyo-cyan hover:text-tokyo-fg'
                )}
                onClick={() => onSplit(tab.id, 'row')}
                disabled={!plugin}
                title={t('plugins.splitRight')}
                aria-label={t('plugins.splitRight')}
              >
                <Columns2 className="h-4 w-4" />
              </button>
              <button
                className={cn(
                  'flex h-7 w-7 items-center justify-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg',
                  'text-tokyo-comment transition-colors hover:border-tokyo-cyan hover:text-tokyo-fg'
                )}
                onClick={() => onSplit(tab.id, 'column')}
                disabled={!plugin}
                title={t('plugins.splitDown')}
                aria-label={t('plugins.splitDown')}
              >
                <Rows2 className="h-4 w-4" />
              </button>
            </>
          )}
          {onClose && (
            <button
              className="icon-button h-7 w-7"
              onClick={onClose}
              aria-label={t('common.close')}
              title={t('common.close')}
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      </header>

      {plugin ? (
        <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
          <PluginPanel
            stateKey={tab.id}
            plugin={plugin}
            sessionId={tab.sessionId}
            sessionType={tab.sessionType}
            variant="workspace"
          />
        </Suspense>
      ) : (
        <div className="flex flex-1 items-center justify-center px-4 text-xs text-tokyo-comment">
          {t('plugins.tabPluginMissing')}
        </div>
      )}
    </div>
  );
}
