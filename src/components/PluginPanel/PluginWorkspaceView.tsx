import { lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { localizedPluginName } from '../../plugins/pluginUtils';
import type { PluginRecord } from '../../plugins/types';
import type { PluginWorkspaceTab } from '../../stores/pluginWorkspaceStore';
import { PluginIcon } from '../PluginIcon';

const PluginPanel = lazy(() => import('./PluginPanel').then((mod) => ({ default: mod.PluginPanel })));

interface PluginWorkspaceViewProps {
  tab: PluginWorkspaceTab;
  plugin: PluginRecord | undefined;
  onClose?: () => void;
}

/**
 * Full-area plugin content. Dragging tabs can still rearrange saved layouts;
 * the workspace intentionally has no split-button toolbar.
 */
export function PluginWorkspaceView({ tab, plugin, onClose }: PluginWorkspaceViewProps) {
  const { t } = useTranslation();
  const pluginName = plugin
    ? localizedPluginName(t, plugin)
    : tab.pluginId;

  return (
    <div className="glass-workspace glass-form plugin-workspace flex h-full min-h-0 flex-col">
      <header className="glass-toolbar flex h-10 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl px-3">
        <PluginIcon name={plugin?.manifest.icon ?? 'plug'} className="h-4 w-4 flex-shrink-0" />
        <h2 className="min-w-0 flex-1 truncate text-sm font-semibold text-tokyo-fg">
          {pluginName}
          <span className="ml-2 font-normal text-tokyo-comment">{tab.serverName}</span>
        </h2>
        <div className="flex flex-shrink-0 items-center gap-1">
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
