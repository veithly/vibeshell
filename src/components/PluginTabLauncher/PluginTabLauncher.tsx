import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Blocks, Loader2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import { isPluginCompatible, localizedPluginName } from '../../plugins/pluginUtils';
import type { PluginRecord, PluginSessionType } from '../../plugins/types';
import { usePluginStore } from '../../stores/pluginStore';
import { PluginIcon } from '../PluginIcon';

interface PluginTabLauncherProps {
  sessionType: PluginSessionType | null;
  onOpenPluginTab: (pluginId: string) => void;
}

/**
 * Toolbar dropdown that lists enabled plugins compatible with the active
 * session and opens the chosen one as a workspace tab.
 */
export function PluginTabLauncher({ sessionType, onOpenPluginTab }: PluginTabLauncherProps) {
  const { t } = useTranslation();
  const plugins = usePluginStore((state) => state.plugins);
  const loading = usePluginStore((state) => state.loading);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const compatiblePlugins = useMemo(
    () => (sessionType ? plugins.filter((plugin) => isPluginCompatible(plugin, sessionType)) : []),
    [plugins, sessionType]
  );

  useEffect(() => {
    if (!open) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <button
        className="icon-button tooltip-button"
        data-tooltip={t('plugins.openTabLauncher')}
        aria-label={t('plugins.openTabLauncher')}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={!sessionType}
        onClick={() => setOpen((current) => !current)}
      >
        <Blocks className="h-4 w-4" />
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-9 z-50 min-w-[220px] rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark py-1 shadow-xl"
        >
          <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wide text-tokyo-comment">
            {t('plugins.openTabLauncher')}
          </div>
          {loading && compatiblePlugins.length === 0 ? (
            <div className="flex items-center gap-2 px-3 py-2 text-xs text-tokyo-comment">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t('plugins.loading')}
            </div>
          ) : compatiblePlugins.length === 0 ? (
            <div className="px-3 py-2 text-xs text-tokyo-comment">
              {t('plugins.noCompatiblePlugins')}
            </div>
          ) : (
            compatiblePlugins.map((plugin: PluginRecord) => (
              <button
                key={plugin.manifest.id}
                role="menuitem"
                className={cn(
                  'flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-tokyo-fg',
                  'transition-colors hover:bg-tokyo-bg-hl cursor-pointer'
                )}
                onClick={() => {
                  setOpen(false);
                  onOpenPluginTab(plugin.manifest.id);
                }}
              >
                <PluginIcon name={plugin.manifest.icon} className="h-4 w-4 flex-shrink-0" />
                <span className="min-w-0 flex-1 truncate">{localizedPluginName(t, plugin)}</span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
