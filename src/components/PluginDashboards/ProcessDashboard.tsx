import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Cpu, Info, Trash2 } from 'lucide-react';
import type { PluginRecord } from '../../plugins/types';
import { parseDelimited } from './parse';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, DashboardModal, ErrorBanner } from './ui';

type SortTab = 'cpu' | 'memory';

const PROCESS_COLUMNS = ['PID', 'PPID', 'User', 'CPU %', 'MEM %', 'RSS', 'Time', 'Command'];

export function ProcessDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [sort, setSort] = useState<SortTab>('cpu');
  const [rows, setRows] = useState<string[][]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<{ pid: string; output: string } | null>(null);
  const [search, setSearch] = useState('');
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const load = useCallback(async (nextSort: SortTab) => {
    clearError();
    setLoading(true);
    const result = await run(nextSort === 'cpu' ? 'top-cpu' : 'top-memory');
    if (result && mountedRef.current) {
      setRows(parseDelimited(result.output).slice(0, 300));
    }
    if (mountedRef.current) setLoading(false);
  }, [clearError, run]);

  useEffect(() => {
    load(sort);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sort]);

  const kill = async (pid: string, command: string) => {
    if (!window.confirm(t('plugins.actionConfirm', { name: `${t('plugins.process.kill')} ${pid} (${command})` }))) {
      return;
    }
    const result = await run('kill', { pid });
    if (result !== null) load(sort);
  };

  const openDetail = async (pid: string) => {
    setDetail({ pid, output: '' });
    const result = await run('process-detail', { pid });
    if (result) setDetail({ pid, output: result.output || t('common.noData') });
  };

  const query = search.trim().toLowerCase();
  const filtered = query === ''
    ? rows
    : rows.filter((row) => row.some((cell) => cell.toLowerCase().includes(query)));

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<Cpu className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.process.title')}
        tabs={[
          { id: 'cpu', label: t('plugins.process.byCpu') },
          { id: 'memory', label: t('plugins.process.byMemory') },
        ]}
        activeTab={sort}
        onTabChange={(next) => setSort(next as SortTab)}
        onRefresh={() => load(sort)}
        refreshing={loading}
        extra={
          <input
            className="mr-1 h-7 w-36 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
            placeholder={t('plugins.process.search')}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        }
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {loading && rows.length === 0 ? (
          <CenterNotice text={t('plugins.process.loading')} loading />
        ) : filtered.length === 0 ? (
          <CenterNotice text={t('common.noData')} />
        ) : (
          <div className="m-2 overflow-x-auto rounded-lg border border-tokyo-bg-hl">
            <table className="w-full border-separate border-spacing-0 text-left text-xs">
              <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                <tr>
                  {PROCESS_COLUMNS.map((column) => (
                    <th key={column} className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">
                      {column}
                    </th>
                  ))}
                  <th className="border-b border-tokyo-bg-hl px-3 py-2 text-right font-medium">
                    {t('plugins.docker.actions')}
                  </th>
                </tr>
              </thead>
              <tbody className="font-mono text-tokyo-fg">
                {filtered.map((row) => {
                  const [pid, ppid, user, cpu, mem, rss, etime, command] = row;
                  return (
                    <tr key={`${pid}-${ppid}-${command}`} className="hover:bg-tokyo-bg-hl/40">
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{pid}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{ppid}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{user}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 tabular-nums text-tokyo-yellow">{cpu}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 tabular-nums text-tokyo-magenta">{mem}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 tabular-nums">{rss}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{etime}</td>
                      <td className="max-w-[260px] border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className="block truncate" title={command}>{command}</span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-right">
                        <div className="inline-flex items-center gap-0.5">
                          <button
                            className="icon-button h-6 w-6"
                            onClick={() => void openDetail(pid)}
                            title={t('plugins.process.detail')}
                            aria-label={t('plugins.process.detail')}
                          >
                            <Info className="h-3.5 w-3.5" />
                          </button>
                          <button
                            className="icon-button h-6 w-6 hover:text-tokyo-red"
                            disabled={runningAction !== null}
                            onClick={() => void kill(pid, command)}
                            title={t('plugins.process.kill')}
                            aria-label={t('plugins.process.kill')}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
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

      {detail !== null && (
        <DashboardModal
          title={`${t('plugins.process.detail')} · PID ${detail.pid}`}
          onClose={() => setDetail(null)}
        >
          {detail.output === '' ? (
            <CenterNotice text={t('plugins.process.loading')} loading />
          ) : (
            <pre className="whitespace-pre-wrap break-words rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
              {detail.output}
            </pre>
          )}
        </DashboardModal>
      )}
    </div>
  );
}
