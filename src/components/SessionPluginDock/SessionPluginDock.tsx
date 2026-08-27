import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { PanelTop, Store, X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { isPluginCompatible, localizedPluginName } from '../../plugins/pluginUtils';
import type { PluginRecord, PluginSessionType } from '../../plugins/types';
import { usePluginStore } from '../../stores/pluginStore';
import { pluginPanelKey } from '../../stores/pluginWorkspaceStore';
import { PluginIcon } from '../PluginIcon';
import { PluginPanel } from '../PluginPanel';

interface SessionPluginDockProps {
  sessionId: string;
  sessionType: PluginSessionType;
  open: boolean;
  onClose: () => void;
  onOpenMarketplace: () => void;
  /** Opens the plugin as a workspace tab beside the session tabs. */
  onOpenPluginTab?: (pluginId: string) => void;
}

export function SessionPluginDock({
  sessionId,
  sessionType,
  open,
  onClose,
  onOpenMarketplace,
  onOpenPluginTab,
}: SessionPluginDockProps) {
  const { t } = useTranslation();
  const plugins = usePluginStore((state) => state.plugins);
  const clearError = usePluginStore((state) => state.clearError);
  const compatiblePlugins = useMemo(
    () => plugins.filter((plugin) => isPluginCompatible(plugin, sessionType)),
    [plugins, sessionType]
  );
  const [activePluginId, setActivePluginId] = useState<string | null>(null);

  const activePlugin: PluginRecord | null = compatiblePlugins.find(
    (plugin) => plugin.manifest.id === activePluginId
  ) ?? compatiblePlugins[0] ?? null;

  useEffect(() => {
    if (!activePlugin || activePlugin.manifest.id === activePluginId) return;
    setActivePluginId(activePlugin.manifest.id);
  }, [activePlugin, activePluginId]);

  if (!open) return null;

  const handleSelectPlugin = (pluginId: string) => {
    clearError();
    setActivePluginId(pluginId);
  };

  const handleClose = () => {
    clearError();
    onClose();
  };

  return (
    <section className="flex-shrink-0 border-t border-tokyo-bg-hl bg-tokyo-bg-dark">
      <div className="flex h-10 items-center border-b border-tokyo-bg-hl px-2">
        <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
          {compatiblePlugins.map((plugin) => (
            <div
              key={plugin.manifest.id}
              className={cn(
                'group/dock-plugin flex h-8 flex-shrink-0 items-center gap-1 rounded-md pl-2.5 pr-1 text-xs transition-colors',
                activePlugin?.manifest.id === plugin.manifest.id
                  ? 'bg-tokyo-bg-hl text-tokyo-fg'
                  : 'text-tokyo-comment hover:bg-tokyo-bg-hl/60 hover:text-tokyo-fg'
              )}
            >
              <button
                className="flex items-center gap-2 py-1"
                onClick={() => handleSelectPlugin(plugin.manifest.id)}
              >
                <PluginIcon name={plugin.manifest.icon} className="h-4 w-4" />
                <span>{localizedPluginName(t, plugin)}</span>
              </button>
              {onOpenPluginTab && (
                <button
                  className={cn(
                    'flex h-6 w-6 items-center justify-center rounded-md text-tokyo-comment',
                    'opacity-0 transition-opacity hover:bg-tokyo-bg-hl hover:text-tokyo-fg',
                    'focus:opacity-100 focus:outline-none focus:ring-1 focus:ring-tokyo-blue group-hover/dock-plugin:opacity-100'
                  )}
                  onClick={() => onOpenPluginTab(plugin.manifest.id)}
                  aria-label={t('plugins.openInTab', { name: localizedPluginName(t, plugin) })}
                  title={t('plugins.openInTab', { name: localizedPluginName(t, plugin) })}
                >
                  <PanelTop className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          ))}
        </div>
        <div className="ml-2 flex flex-shrink-0 items-center gap-1 border-l border-tokyo-bg-hl pl-2">
          <button
            className="icon-button tooltip-button h-7 w-7"
            data-tooltip={t('plugins.marketplace')}
            aria-label={t('plugins.marketplace')}
            onClick={onOpenMarketplace}
          >
            <Store className="h-4 w-4" />
          </button>
          <button
            className="icon-button tooltip-button h-7 w-7"
            data-tooltip={t('common.close')}
            aria-label={t('common.close')}
            onClick={handleClose}
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </div>

      {!activePlugin ? (
        <div className="flex h-40 flex-col items-center justify-center gap-3 text-tokyo-comment">
          <p className="text-sm">{t('plugins.noCompatiblePlugins')}</p>
          <button
            className="flex h-8 items-center gap-2 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg hover:border-tokyo-cyan"
            onClick={onOpenMarketplace}
          >
            <Store className="h-4 w-4" />
            {t('plugins.openMarketplace')}
          </button>
        </div>
      ) : (
        <PluginPanel
          key={`${sessionId}:${activePlugin.manifest.id}`}
          stateKey={pluginPanelKey(sessionId, activePlugin.manifest.id)}
          plugin={activePlugin}
          sessionId={sessionId}
          sessionType={sessionType}
          variant="dock"
        />
      )}

    </section>
  );
}