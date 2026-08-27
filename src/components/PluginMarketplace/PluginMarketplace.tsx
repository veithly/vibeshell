import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  AlertTriangle,
  Check,
  Download,
  FileDown,
  Loader2,
  PackageOpen,
  Search,
  ShieldCheck,
  Trash2,
  Upload,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { usePluginStore } from '../../stores/pluginStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { PluginIcon } from '../PluginIcon';

type InstallFilter = 'all' | 'installed';

function pluginText(
  t: ReturnType<typeof useTranslation>['t'],
  plugin: PluginRecord,
  field: 'name' | 'description'
): string {
  return t(`plugins.catalog.${plugin.manifest.id}.${field}`, {
    defaultValue: plugin.manifest[field],
  });
}

export function PluginMarketplace() {
  const { t } = useTranslation();
  const {
    plugins,
    loading,
    initialized,
    operationId,
    error,
    fetchPlugins,
    installPlugin,
    importPlugin,
    exportPlugin,
    uninstallPlugin,
    setPluginEnabled,
    clearError,
  } = usePluginStore();
  const { success: notifySuccess } = useNotificationStore();
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState('all');
  const [installFilter, setInstallFilter] = useState<InstallFilter>('all');

  useEffect(() => {
    if (!initialized) void fetchPlugins();
  }, [fetchPlugins, initialized]);

  const categories = useMemo(() => {
    const counts = new Map<string, number>();
    for (const plugin of plugins) {
      counts.set(plugin.manifest.category, (counts.get(plugin.manifest.category) ?? 0) + 1);
    }
    return Array.from(counts.entries()).sort(([left], [right]) => left.localeCompare(right));
  }, [plugins]);

  const visiblePlugins = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return plugins.filter((plugin) => {
      if (category !== 'all' && plugin.manifest.category !== category) return false;
      if (installFilter === 'installed' && !plugin.installed) return false;
      if (!query) return true;
      return [
        pluginText(t, plugin, 'name'),
        pluginText(t, plugin, 'description'),
        plugin.manifest.author,
        plugin.manifest.category,
      ].some((value) => value.toLocaleLowerCase().includes(query));
    });
  }, [category, installFilter, plugins, search, t]);

  const installedCount = plugins.filter((plugin) => plugin.installed).length;

  const handleEnabledChange = async (plugin: PluginRecord, enabled: boolean) => {
    if (
      enabled
      && plugin.source === 'external'
      && !window.confirm(t('plugins.externalEnableConfirm', { name: plugin.manifest.name }))
    ) {
      return;
    }
    await setPluginEnabled(plugin.manifest.id, enabled);
  };

  const handleUninstall = async (plugin: PluginRecord) => {
    if (!window.confirm(t('plugins.uninstallConfirm', { name: pluginText(t, plugin, 'name') }))) {
      return;
    }
    await uninstallPlugin(plugin.manifest.id);
  };

  const handleExport = async (plugin: PluginRecord) => {
    const path = await exportPlugin(plugin.manifest.id);
    if (path) {
      notifySuccess(t('plugins.exportedTitle'), path, 8000);
    }
  };

  return (
    <div className="mx-auto flex h-full w-full max-w-[1480px] overflow-hidden">
      <aside className="hidden w-56 flex-shrink-0 border-r border-tokyo-bg-hl bg-tokyo-bg-dark/60 px-3 py-5 md:flex md:flex-col">
        <div className="px-2 pb-5">
          <div className="text-[11px] font-semibold uppercase text-tokyo-comment">
            {t('plugins.registry')}
          </div>
          <div className="mt-2 flex items-end gap-2">
            <span className="text-3xl font-semibold tabular-nums text-tokyo-fg">{plugins.length}</span>
            <span className="pb-1 text-xs text-tokyo-comment">
              {t('plugins.installedCount', { count: installedCount })}
            </span>
          </div>
        </div>

        <nav className="space-y-1" aria-label={t('plugins.categories')}>
          <button
            className={cn('plugin-category-button', category === 'all' && 'is-active')}
            onClick={() => setCategory('all')}
          >
            <span>{t('plugins.allCategories')}</span>
            <span className="tabular-nums text-tokyo-comment">{plugins.length}</span>
          </button>
          {categories.map(([item, count]) => (
            <button
              key={item}
              className={cn('plugin-category-button', category === item && 'is-active')}
              onClick={() => setCategory(item)}
            >
              <span>{t(`plugins.category.${item}`, { defaultValue: item })}</span>
              <span className="tabular-nums text-tokyo-comment">{count}</span>
            </button>
          ))}
        </nav>

        <div className="mt-auto border-t border-tokyo-bg-hl px-2 pt-4 text-xs leading-5 text-tokyo-comment">
          <div className="mb-1 flex items-center gap-2 font-medium text-tokyo-fg">
            <ShieldCheck className="h-4 w-4 text-tokyo-green" />
            {t('plugins.hostRendered')}
          </div>
          {t('plugins.hostRenderedDesc')}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex flex-wrap items-center gap-3 border-b border-tokyo-bg-hl px-4 py-3 lg:px-6">
          <label className="relative min-w-[220px] flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-tokyo-comment" />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t('plugins.search')}
              className="h-9 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg pl-9 pr-3 text-sm text-tokyo-fg outline-none transition-colors focus:border-tokyo-cyan"
            />
          </label>

          <div className="flex h-9 rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-0.5">
            {(['all', 'installed'] as InstallFilter[]).map((filter) => (
              <button
                key={filter}
                className={cn(
                  'min-w-20 rounded px-3 text-xs text-tokyo-comment transition-colors',
                  installFilter === filter && 'bg-tokyo-bg-hl text-tokyo-fg'
                )}
                onClick={() => setInstallFilter(filter)}
              >
                {t(`plugins.filter.${filter}`)}
              </button>
            ))}
          </div>

          <button
            className="flex h-9 items-center gap-2 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-sm text-tokyo-fg transition-colors hover:border-tokyo-cyan hover:bg-tokyo-bg-hl disabled:opacity-50"
            onClick={() => void importPlugin()}
            disabled={operationId === 'import'}
          >
            {operationId === 'import'
              ? <Loader2 className="h-4 w-4 animate-spin" />
              : <Upload className="h-4 w-4" />}
            {t('plugins.importManifest')}
          </button>
        </div>

        {error && (
          <div className="mx-4 mt-4 flex items-center gap-2 rounded-md border border-tokyo-red/30 bg-tokyo-red/10 px-3 py-2 text-sm text-tokyo-red lg:mx-6">
            <AlertTriangle className="h-4 w-4 flex-shrink-0" />
            <span className="min-w-0 flex-1 truncate">{error}</span>
            <button className="text-xs underline" onClick={clearError}>{t('common.close')}</button>
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto p-4 lg:p-6">
          {loading && plugins.length === 0 ? (
            <div className="flex h-full items-center justify-center text-tokyo-comment">
              <Loader2 className="mr-2 h-5 w-5 animate-spin" />
              {t('plugins.loading')}
            </div>
          ) : visiblePlugins.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center text-center text-tokyo-comment">
              <PackageOpen className="mb-3 h-8 w-8" />
              <p className="text-sm">{t('plugins.noResults')}</p>
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-3 xl:grid-cols-2 2xl:grid-cols-3">
              {visiblePlugins.map((plugin) => {
                const pluginId = plugin.manifest.id;
                const busy = operationId === pluginId;
                return (
                  <article
                    key={pluginId}
                    className="plugin-market-card flex min-h-[238px] flex-col border border-tokyo-bg-hl bg-tokyo-bg p-4"
                  >
                    <div className="flex items-start gap-3">
                      <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark text-tokyo-cyan">
                        <PluginIcon name={plugin.manifest.icon} className="h-5 w-5" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <h2 className="truncate text-sm font-semibold text-tokyo-fg">
                            {pluginText(t, plugin, 'name')}
                          </h2>
                          {plugin.installed && (
                            <Check className="h-4 w-4 flex-shrink-0 text-tokyo-green" />
                          )}
                        </div>
                        <div className="mt-1 flex items-center gap-2 text-[11px] text-tokyo-comment">
                          <span>{plugin.manifest.author}</span>
                          <span>v{plugin.manifest.version}</span>
                          <span className={cn(
                            'rounded border px-1.5 py-0.5',
                            plugin.source === 'external'
                              ? 'border-tokyo-yellow/40 text-tokyo-yellow'
                              : 'border-tokyo-bg-hl'
                          )}>
                            {t(`plugins.source.${plugin.source}`)}
                          </span>
                        </div>
                      </div>
                    </div>

                    <p className="mt-3 min-h-[60px] overflow-hidden text-sm leading-5 text-tokyo-comment">
                      {pluginText(t, plugin, 'description')}
                    </p>

                    <div className="mt-3 flex flex-wrap gap-1.5 text-[10px]">
                      <span className="rounded border border-tokyo-bg-hl bg-tokyo-bg-dark px-2 py-1 text-tokyo-comment">
                        {plugin.manifest.sessionTypes.map((type) => type.toUpperCase()).join(' + ')}
                      </span>
                      {plugin.manifest.permissions.map((permission) => (
                        <span key={permission} className="rounded border border-tokyo-bg-hl px-2 py-1 text-tokyo-comment">
                          {t(`plugins.permission.${permission}`)}
                        </span>
                      ))}
                    </div>

                    <div className="mt-auto flex h-9 items-center justify-between border-t border-tokyo-bg-hl pt-3">
                      {plugin.installed ? (
                        <>
                          <label className="flex cursor-pointer items-center gap-2 text-xs text-tokyo-comment">
                            <input
                              type="checkbox"
                              className="plugin-toggle-input"
                              checked={plugin.enabled}
                              disabled={busy}
                              onChange={(event) => void handleEnabledChange(plugin, event.target.checked)}
                            />
                            <span>{plugin.enabled ? t('plugins.enabled') : t('plugins.disabled')}</span>
                          </label>
                          <div className="flex items-center gap-1">
                            <button
                              className="icon-button tooltip-button h-8 w-8 text-tokyo-comment hover:text-tokyo-fg"
                              data-tooltip={t('plugins.export')}
                              aria-label={t('plugins.export')}
                              disabled={operationId === `export:${pluginId}`}
                              onClick={() => void handleExport(plugin)}
                            >
                              {operationId === `export:${pluginId}`
                                ? <Loader2 className="h-4 w-4 animate-spin" />
                                : <FileDown className="h-4 w-4" />}
                            </button>
                            <button
                              className="icon-button tooltip-button h-8 w-8 text-tokyo-comment hover:text-tokyo-red"
                              data-tooltip={t('plugins.uninstall')}
                              aria-label={t('plugins.uninstall')}
                              disabled={busy}
                              onClick={() => void handleUninstall(plugin)}
                            >
                              {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Trash2 className="h-4 w-4" />}
                            </button>
                          </div>
                        </>
                      ) : (
                        <>
                          <button
                            className="icon-button tooltip-button h-8 w-8 text-tokyo-comment hover:text-tokyo-fg"
                            data-tooltip={t('plugins.exportTemplate')}
                            aria-label={t('plugins.exportTemplate')}
                            disabled={operationId === `export:${pluginId}`}
                            onClick={() => void handleExport(plugin)}
                          >
                            {operationId === `export:${pluginId}`
                              ? <Loader2 className="h-4 w-4 animate-spin" />
                              : <FileDown className="h-4 w-4" />}
                          </button>
                          <button
                            className="flex h-8 items-center gap-2 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent transition-opacity hover:opacity-90 disabled:opacity-50"
                            onClick={() => void installPlugin(pluginId)}
                            disabled={busy}
                          >
                            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
                            {t('plugins.install')}
                          </button>
                        </>
                      )}
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
