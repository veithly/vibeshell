import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Play, RotateCw, Settings2, Square } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, DashboardModal, ErrorBanner } from './ui';

interface ServiceUnit {
  unit: string;
  load: string;
  active: string;
  sub: string;
  description: string;
}

function parseServices(output: string): ServiceUnit[] {
  const trimmed = output.trim();
  if (trimmed.startsWith('[') || trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed) as Array<Record<string, string>>;
      return parsed.map((entry) => ({
        unit: entry.unit ?? '',
        load: entry.load ?? '',
        active: entry.active ?? '',
        sub: entry.sub ?? '',
        description: entry.description ?? '',
      }));
    } catch {
      // fall through to plain-text parsing
    }
  }
  return output
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.endsWith('.service'))
    .map((line) => {
      const fields = line.split(/\s{2,}/);
      const [unit, load, active, sub, ...rest] = fields;
      return {
        unit: unit ?? '',
        load: load ?? '',
        active: active ?? '',
        sub: sub ?? '',
        description: rest.join('  ').replace(/^[-•]\s*/, ''),
      };
    });
}

export function ServicesDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [services, setServices] = useState<ServiceUnit[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState('');
  const [onlyActive, setOnlyActive] = useState(false);
  const [statusFor, setStatusFor] = useState<string | null>(null);
  const [statusOutput, setStatusOutput] = useState<string | null>(null);

  const load = useCallback(async () => {
    clearError();
    setLoading(true);
    const result = await run('list');
    if (result) setServices(parseServices(result.output));
    setLoading(false);
  }, [clearError, run]);

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const lifecycle = async (
    action: 'start' | 'stop' | 'restart',
    unit: string
  ) => {
    if (!window.confirm(t('plugins.actionConfirm', { name: `systemctl ${action} ${unit}` }))) {
      return;
    }
    await run(action, { service: unit });
    await load();
  };

  const openStatus = async (unit: string) => {
    setStatusFor(unit);
    setStatusOutput(null);
    const result = await run('status', { service: unit });
    if (result) setStatusOutput(result.output || t('common.noData'));
  };

  const query = search.trim().toLowerCase();
  const filtered = useMemo(
    () => services.filter((service) =>
      (!onlyActive || service.active === 'active')
      && (query === ''
        || service.unit.toLowerCase().includes(query)
        || service.description.toLowerCase().includes(query))
    ),
    [services, onlyActive, query]
  );

  const activeCount = services.filter((service) => service.active === 'active').length;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<Settings2 className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.services.title')}
        badge={`${activeCount}/${services.length}`}
        onRefresh={() => load()}
        refreshing={loading}
        extra={
          <>
            <input
              className="mr-1 h-7 w-40 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
              placeholder={t('plugins.services.search')}
              value={search}
              onChange={(event) => setSearch(event.target.value)}
            />
            <label className="mr-1 flex items-center gap-1.5 text-xs text-tokyo-comment">
              <input
                type="checkbox"
                className="plugin-toggle-input"
                checked={onlyActive}
                onChange={(event) => setOnlyActive(event.target.checked)}
              />
              {t('plugins.services.activeOnly')}
            </label>
          </>
        }
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {loading && services.length === 0 ? (
          <CenterNotice text={t('plugins.services.loading')} loading />
        ) : filtered.length === 0 ? (
          <CenterNotice text={t('common.noData')} />
        ) : (
          <div className="m-2 overflow-x-auto rounded-lg border border-tokyo-bg-hl">
            <table className="w-full border-separate border-spacing-0 text-left text-xs">
              <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                <tr>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.services.service')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.services.state')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.services.description')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 text-right font-medium">{t('plugins.docker.actions')}</th>
                </tr>
              </thead>
              <tbody className="text-tokyo-fg">
                {filtered.map((service) => {
                  const isActive = service.active === 'active';
                  return (
                    <tr key={service.unit} className="hover:bg-tokyo-bg-hl/40">
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 font-mono">
                        <button
                          className="hover:text-tokyo-cyan hover:underline"
                          onClick={() => void openStatus(service.unit)}
                        >
                          {service.unit}
                        </button>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span
                          className={cn(
                            'inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[10px]',
                            isActive
                              ? 'border-tokyo-green/40 text-tokyo-green'
                              : 'border-tokyo-bg-hl text-tokyo-comment'
                          )}
                        >
                          <span className={cn('h-1.5 w-1.5 rounded-full', isActive ? 'bg-tokyo-green' : 'bg-tokyo-comment')} />
                          {service.active} · {service.sub}
                        </span>
                      </td>
                      <td className="max-w-[280px] border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className="block truncate" title={service.description}>
                          {service.description || '-'}
                        </span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-right">
                        <div className="inline-flex items-center gap-0.5">
                          {isActive ? (
                            <button
                              className="icon-button h-6 w-6 hover:text-tokyo-red"
                              disabled={runningAction !== null}
                              onClick={() => void lifecycle('stop', service.unit)}
                              title={t('plugins.services.stop')}
                              aria-label={t('plugins.services.stop')}
                            >
                              <Square className="h-3.5 w-3.5" />
                            </button>
                          ) : (
                            <button
                              className="icon-button h-6 w-6 hover:text-tokyo-green"
                              disabled={runningAction !== null}
                              onClick={() => void lifecycle('start', service.unit)}
                              title={t('plugins.services.start')}
                              aria-label={t('plugins.services.start')}
                            >
                              <Play className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button
                            className="icon-button h-6 w-6 hover:text-tokyo-yellow"
                            disabled={runningAction !== null}
                            onClick={() => void lifecycle('restart', service.unit)}
                            title={t('plugins.services.restart')}
                            aria-label={t('plugins.services.restart')}
                          >
                            <RotateCw className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {statusFor !== null && (
        <DashboardModal
          title={statusFor}
          onClose={() => setStatusFor(null)}
          wide
        >
          {statusOutput === null ? (
            <CenterNotice text={t('plugins.services.loading')} loading />
          ) : (
            <pre className="whitespace-pre-wrap break-words rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
              {statusOutput}
            </pre>
          )}
        </DashboardModal>
      )}
    </div>
  );
}
