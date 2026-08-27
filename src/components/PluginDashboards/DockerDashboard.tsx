import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  FileText,
  Loader2,
  Play,
  RotateCw,
  ScanSearch,
  Square,
} from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { parseDelimited } from './parse';
import { findAction, usePluginAction } from './usePluginAction';
import {
  CenterNotice,
  DashboardHeader,
  DashboardModal,
  DataGrid,
  ErrorBanner,
} from './ui';

interface DockerContainer {
  id: string;
  name: string;
  image: string;
  status: string;
  ports: string;
}

interface DockerStats {
  cpu: string;
  memory: string;
}

type DockerTab = 'containers' | 'images' | 'volumes' | 'networks';

const REFRESH_OPTIONS = [
  { value: 0, labelKey: 'plugins.docker.refreshOff' },
  { value: 10, labelKey: 'plugins.docker.refresh10s' },
  { value: 30, labelKey: 'plugins.docker.refresh30s' },
  { value: 60, labelKey: 'plugins.docker.refresh60s' },
];

export function DockerDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<DockerTab>('containers');
  const [containers, setContainers] = useState<DockerContainer[]>([]);
  const [stats, setStats] = useState<Record<string, DockerStats>>({});
  const [listRows, setListRows] = useState<{ columns: string[]; rows: string[][] }>({ columns: [], rows: [] });
  const [version, setVersion] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshSeconds, setRefreshSeconds] = useState(0);
  const [logsFor, setLogsFor] = useState<string | null>(null);
  const [logsOutput, setLogsOutput] = useState<string | null>(null);
  const [inspectFor, setInspectFor] = useState<string | null>(null);
  const [inspectOutput, setInspectOutput] = useState<string | null>(null);
  const [containerFilter, setContainerFilter] = useState<'all' | 'running' | 'stopped'>('all');
  const [search, setSearch] = useState('');
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const loadContainers = useCallback(async () => {
    setLoading(true);
    const result = await run('containers');
    if (result && mountedRef.current) {
      const rows = parseDelimited(result.output);
      setContainers(
        rows.map(([id, name, image, status, ports]) => ({
          id: id ?? '',
          name: name ?? '',
          image: image ?? '',
          status: status ?? '',
          ports: ports ?? '',
        }))
      );
    }
    const statsResult = await run('stats');
    if (statsResult && mountedRef.current) {
      const map: Record<string, DockerStats> = {};
      for (const [name, cpu, memory] of parseDelimited(statsResult.output)) {
        if (name) map[name] = { cpu: cpu ?? '-', memory: memory ?? '-' };
      }
      setStats(map);
    }
    if (mountedRef.current) setLoading(false);
  }, [run]);

  const loadList = useCallback(async (actionId: string, columns: string[]) => {
    setLoading(true);
    const result = await run(actionId);
    if (result && mountedRef.current) {
      setListRows({ columns, rows: parseDelimited(result.output) });
    }
    if (mountedRef.current) setLoading(false);
  }, [run]);

  const loadTab = useCallback((nextTab: DockerTab) => {
    clearError();
    switch (nextTab) {
      case 'containers':
        void loadContainers();
        break;
      case 'images':
        void loadList('images', [t('plugins.docker.image'), 'ID', t('plugins.docker.size'), t('plugins.docker.created')]);
        break;
      case 'volumes':
        void loadList('volumes', [t('plugins.docker.volume'), 'Driver', 'Scope']);
        break;
      case 'networks':
        void loadList('networks', [t('plugins.docker.network'), 'Driver', 'Scope']);
        break;
    }
  }, [clearError, loadContainers, loadList, t]);

  useEffect(() => {
    void run('version').then((result) => {
      if (result && mountedRef.current) setVersion(result.output.trim() || null);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    loadTab(tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  useEffect(() => {
    if (refreshSeconds === 0 || tab !== 'containers') return;
    const interval = window.setInterval(() => {
      void loadContainers();
    }, refreshSeconds * 1000);
    return () => window.clearInterval(interval);
  }, [refreshSeconds, tab, loadContainers]);

  const running = useMemo(() => containers.filter((c) => c.status.startsWith('Up')).length, [containers]);

  const query = search.trim().toLowerCase();
  const visibleContainers = useMemo(
    () => containers.filter((container) => {
      const isUp = container.status.startsWith('Up');
      if (containerFilter === 'running' && !isUp) return false;
      if (containerFilter === 'stopped' && isUp) return false;
      if (query === '') return true;
      return (
        container.name.toLowerCase().includes(query)
        || container.image.toLowerCase().includes(query)
        || container.status.toLowerCase().includes(query)
      );
    }),
    [containers, containerFilter, query]
  );

  const lifecycle = async (actionId: 'start-container' | 'stop-container' | 'restart-container', name: string) => {
    const action = findAction(plugin, actionId);
    if (action && !window.confirm(t('plugins.actionConfirm', { name: `${action.name}: ${name}` }))) {
      return;
    }
    await run(actionId, { container: name });
    await loadContainers();
  };

  const openLogs = async (name: string) => {
    setLogsFor(name);
    setLogsOutput(null);
    const result = await run('logs', { container: name });
    if (result && mountedRef.current) setLogsOutput(result.output || t('common.noData'));
  };

  const openInspect = async (name: string) => {
    setInspectFor(name);
    setInspectOutput(null);
    const result = await run('inspect', { container: name });
    if (result && mountedRef.current) {
      try {
        setInspectOutput(JSON.stringify(JSON.parse(result.output), null, 2));
      } catch {
        setInspectOutput(result.output);
      }
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<ScanSearch className="h-4 w-4 text-tokyo-cyan" />}
        title="Docker"
        badge={version ? `v${version}` : null}
        tabs={[
          { id: 'containers', label: `${t('plugins.docker.containers')} (${running}/${containers.length})` },
          { id: 'images', label: t('plugins.docker.images') },
          { id: 'volumes', label: t('plugins.docker.volumes') },
          { id: 'networks', label: t('plugins.docker.networks') },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as DockerTab)}
        onRefresh={() => loadTab(tab)}
        refreshing={loading}
        extra={
          tab === 'containers' ? (
            <>
              <input
                className="mr-1 h-7 w-36 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                placeholder={t('plugins.docker.search')}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
              <div className="mr-1 flex h-7 rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
                {([
                  ['all', t('plugins.docker.filterAll', { count: containers.length })],
                  ['running', t('plugins.docker.filterRunning', { count: running })],
                  ['stopped', t('plugins.docker.filterStopped', { count: containers.length - running })],
                ] as Array<['all' | 'running' | 'stopped', string]>).map(([id, label]) => (
                  <button
                    key={id}
                    className={cn(
                      'rounded px-2 text-xs transition-colors',
                      containerFilter === id ? 'bg-tokyo-bg-hl text-tokyo-fg' : 'text-tokyo-comment hover:text-tokyo-fg'
                    )}
                    onClick={() => setContainerFilter(id)}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <select
                className="mr-1 h-7 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none"
                value={refreshSeconds}
                onChange={(event) => setRefreshSeconds(Number(event.target.value))}
                aria-label={t('plugins.docker.autoRefresh')}
              >
                {REFRESH_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {t(option.labelKey)}
                  </option>
                ))}
              </select>
            </>
          ) : undefined
        }
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {loading && containers.length === 0 && listRows.rows.length === 0 ? (
          <CenterNotice text={t('plugins.docker.loading')} loading />
        ) : tab === 'containers' ? (
          visibleContainers.length === 0 ? (
            <CenterNotice text={t('plugins.docker.noContainers')} />
          ) : (
            <div className="m-2 overflow-x-auto rounded-lg border border-tokyo-bg-hl">
            <table className="w-full border-separate border-spacing-0 text-left text-xs">
              <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                <tr>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.docker.container')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.docker.image')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.docker.statusHeader')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">CPU</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.docker.memory')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.docker.ports')}</th>
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 text-right font-medium">{t('plugins.docker.actions')}</th>
                </tr>
              </thead>
              <tbody className="text-tokyo-fg">
                {visibleContainers.map((container) => {
                  const isUp = container.status.startsWith('Up');
                  return (
                    <tr key={container.id || container.name} className="hover:bg-tokyo-bg-hl/40">
                      <td className="max-w-[180px] border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <div className="flex items-center gap-2">
                          <span
                            className={cn(
                              'h-2 w-2 flex-shrink-0 rounded-full',
                              isUp ? 'bg-tokyo-green' : 'bg-tokyo-comment'
                            )}
                            title={isUp ? t('plugins.docker.running') : t('plugins.docker.stopped')}
                          />
                          <button
                            className="truncate font-medium hover:text-tokyo-cyan hover:underline"
                            title={container.name}
                            onClick={() => void openInspect(container.name)}
                          >
                            {container.name}
                          </button>
                        </div>
                      </td>
                      <td className="max-w-[200px] border-b border-tokyo-bg-hl/60 px-3 py-1.5 font-mono">
                        <span className="block truncate" title={container.image}>{container.image}</span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className={isUp ? 'text-tokyo-green' : 'text-tokyo-comment'}>{container.status}</span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 tabular-nums">
                        {stats[container.name]?.cpu ?? '-'}
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 tabular-nums">
                        {stats[container.name]?.memory ?? '-'}
                      </td>
                      <td className="max-w-[220px] border-b border-tokyo-bg-hl/60 px-3 py-1.5 font-mono">
                        <span className="block truncate" title={container.ports}>{container.ports || '-'}</span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-right">
                        <div className="inline-flex items-center gap-0.5">
                          <button
                            className="icon-button h-6 w-6"
                            onClick={() => void openLogs(container.name)}
                            title={t('plugins.docker.logs')}
                            aria-label={t('plugins.docker.logs')}
                          >
                            <FileText className="h-3.5 w-3.5" />
                          </button>
                          {isUp ? (
                            <button
                              className="icon-button h-6 w-6 hover:text-tokyo-red"
                              disabled={runningAction !== null}
                              onClick={() => void lifecycle('stop-container', container.name)}
                              title={t('plugins.docker.stop')}
                              aria-label={t('plugins.docker.stop')}
                            >
                              {runningAction === 'stop-container' && logsFor === null
                                ? <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                : <Square className="h-3.5 w-3.5" />}
                            </button>
                          ) : (
                            <button
                              className="icon-button h-6 w-6 hover:text-tokyo-green"
                              disabled={runningAction !== null}
                              onClick={() => void lifecycle('start-container', container.name)}
                              title={t('plugins.docker.start')}
                              aria-label={t('plugins.docker.start')}
                            >
                              <Play className="h-3.5 w-3.5" />
                            </button>
                          )}
                          <button
                            className="icon-button h-6 w-6 hover:text-tokyo-yellow"
                            disabled={runningAction !== null}
                            onClick={() => void lifecycle('restart-container', container.name)}
                            title={t('plugins.docker.restart')}
                            aria-label={t('plugins.docker.restart')}
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
          )
        ) : (
          <DataGrid columns={listRows.columns} rows={listRows.rows} emptyText={t('common.noData')} />
        )}
      </div>

      {logsFor !== null && (
        <DashboardModal
          title={`${t('plugins.docker.logs')} · ${logsFor}`}
          onClose={() => setLogsFor(null)}
          wide
        >
          {logsOutput === null ? (
            <CenterNotice text={t('plugins.docker.loading')} loading />
          ) : (
            <pre className="whitespace-pre-wrap break-words rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
              {logsOutput}
            </pre>
          )}
          <div className="mt-2 flex justify-end">
            <button
              className="flex h-7 items-center gap-1.5 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2.5 text-xs text-tokyo-fg hover:border-tokyo-cyan"
              onClick={() => void openLogs(logsFor)}
            >
              <RotateCw className="h-3.5 w-3.5" />
              {t('plugins.runAgain')}
            </button>
          </div>
        </DashboardModal>
      )}

      {inspectFor !== null && (
        <DashboardModal
          title={`${t('plugins.docker.inspect')} · ${inspectFor}`}
          onClose={() => setInspectFor(null)}
          wide
        >
          {inspectOutput === null ? (
            <CenterNotice text={t('plugins.docker.loading')} loading />
          ) : (
            <pre className="whitespace-pre-wrap break-words rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
              {inspectOutput}
            </pre>
          )}
        </DashboardModal>
      )}
    </div>
  );
}
