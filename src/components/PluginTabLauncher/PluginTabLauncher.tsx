import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Blocks, Loader2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import { focusMenuItem, handleMenuNavigation } from '../../lib/menuNavigation';
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
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

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
      if (event.key === 'Escape') { setOpen(false); triggerRef.current?.focus(); }
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [open]);

  useEffect(() => { if (open) focusMenuItem(menuRef.current); }, [open, loading]);

  return (
    <div ref={rootRef} className="relative">
      <button
        ref={triggerRef}
        className={cn('icon-button tooltip-button', open && 'is-active')}
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
          ref={menuRef}
          role="menu"
          aria-label={t('plugins.openTabLauncher')}
          className="glass-menu glass-menu-popover absolute right-0 top-full z-50 mt-2 w-64"
          onKeyDown={handleMenuNavigation}
        >
          <div className="glass-menu-heading">
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
                className="glass-menu-item"
                onClick={() => {
                  setOpen(false);
                  triggerRef.current?.focus();
                  onOpenPluginTab(plugin.manifest.id);
                }}
              >
                <span className="glass-menu-icon"><PluginIcon name={plugin.manifest.icon} className="h-4 w-4" /></span>
                <span className="glass-menu-label">{localizedPluginName(t, plugin)}</span>
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}
