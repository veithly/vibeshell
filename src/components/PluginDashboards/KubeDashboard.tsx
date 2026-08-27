import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Boxes, FileText } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, DashboardModal, ErrorBanner } from './ui';

type KubeTab = 'pods' | 'deployments' | 'events' | 'contexts';

interface KubeRow {
  namespace?: string;
  name: string;
  status: string;
  cells: string[];
}

function podStatusTone(status: string) {
  if (status === 'Running') return 'text-tokyo-green';
  if (status.startsWith('Crash') || status === 'Error' || status === 'Failed') return 'text-tokyo-red';
  if (status === 'Pending' || status.startsWith('Init')) return 'text-tokyo-yellow';
  return 'text-tokyo-comment';
}

function parseWideTable(output: string, statusIndex: number, nameIndex: number, namespaceIndex?: number): KubeRow[] {
  const lines = output.split('\n').map((line) => line.replace(/\r$/, '')).filter((line) => line.trim().length > 0);
  if (lines.length < 2) return [];
  const header = lines[0].trim().split(/\s{2,}/);
  return lines.slice(1).map((line) => {
    const cells = line.trim().split(/\s{2,}/);
    return {
      namespace: namespaceIndex !== undefined ? cells[namespaceIndex] : undefined,
      name: cells[nameIndex] ?? cells[0] ?? '',
      status: cells[statusIndex] ?? '',
      cells,
    };
  }).filter((row) => row.cells.length === header.length || row.cells.length > 0);
}

export function KubeDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<KubeTab>('pods');
  const [header, setHeader] = useState<string[]>([]);
  const [rows, setRows] = useState<KubeRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [logsFor, setLogsFor] = useState<{ namespace: string; pod: string } | null>(null);
  const [logsOutput, setLogsOutput] = useState<string | null>(null);

  const load = useCallback(async (nextTab: KubeTab) => {
    clearError();
    setLoading(true);
    setRows(null);
    const actionId = nextTab === 'pods'
      ? 'pods'
      : nextTab === 'deployments'
        ? 'deployments'
        : nextTab === 'events'
          ? 'events'
          : 'contexts';
    const result = await run(actionId);
    if (result) {
      const lines = result.output.split('\n').filter((line) => line.trim().length > 0);
      setHeader(lines[0]?.trim().split(/\s{2,}/) ?? []);
      if (nextTab === 'pods') {
        // NAMESPACE NAME READY STATUS RESTARTS AGE …
        setRows(parseWideTable(result.output, 3, 1, 0));
      } else if (nextTab === 'deployments') {
        // NAMESPACE NAME READY UP-TO-DATE AVAILABLE AGE …
        setRows(parseWideTable(result.output, 2, 1, 0));
      } else if (nextTab === 'events') {
        setRows(lines.slice(1).map((line) => ({ name: '', status: '', cells: [line.trim()] })));
      } else {
        setRows(lines.slice(1).map((line) => ({ name: '', status: '', cells: line.trim().split(/\s{2,}/) })));
      }
    }
    setLoading(false);
  }, [clearError, run]);

  useEffect(() => {
    load(tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  const openPodLogs = async (row: KubeRow) => {
    const namespace = row.namespace ?? 'default';
    const pod = row.name;
    if (!pod) return;
    setLogsFor({ namespace, pod });
    setLogsOutput(null);
    const result = await run('pod-logs', { namespace, pod });
    if (result) setLogsOutput(result.output || t('common.noData'));
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<Boxes className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.kube.title')}
        tabs={[
          { id: 'pods', label: t('plugins.kube.pods') },
          { id: 'deployments', label: t('plugins.kube.deployments') },
          { id: 'events', label: t('plugins.kube.events') },
          { id: 'contexts', label: t('plugins.kube.contexts') },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as KubeTab)}
        onRefresh={() => load(tab)}
        refreshing={loading}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {loading && rows === null ? (
          <CenterNotice text={t('plugins.kube.loading')} loading />
        ) : (rows ?? []).length === 0 ? (
          <CenterNotice text={t('common.noData')} />
        ) : (
          <div className="m-2 overflow-x-auto rounded-lg border border-tokyo-bg-hl">
            <table className="w-full border-separate border-spacing-0 text-left text-xs">
              <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                <tr>
                  {(header.length > 0 ? header : ['']).map((column, index) => (
                    <th key={index} className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">
                      {column}
                    </th>
                  ))}
                  {tab === 'pods' && (
                    <th className="border-b border-tokyo-bg-hl px-3 py-2 text-right font-medium">
                      {t('plugins.docker.actions')}
                    </th>
                  )}
                </tr>
              </thead>
              <tbody className="font-mono text-tokyo-fg">
                {(rows ?? []).map((row, index) => (
                  <tr key={`${row.name}-${index}`} className="hover:bg-tokyo-bg-hl/40">
                    {row.cells.map((cell, cellIndex) => (
                      <td
                        key={cellIndex}
                        className={cn(
                          'max-w-[220px] border-b border-tokyo-bg-hl/60 px-3 py-1.5',
                          tab === 'pods' && cellIndex === 3 && podStatusTone(row.status)
                        )}
                      >
                        <span className="block truncate" title={cell}>{cell}</span>
                      </td>
                    ))}
                    {tab === 'pods' && (
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-right">
                        <button
                          className="icon-button h-6 w-6"
                          onClick={() => void openPodLogs(row)}
                          title={t('plugins.docker.logs')}
                          aria-label={t('plugins.docker.logs')}
                        >
                          <FileText className="h-3.5 w-3.5" />
                        </button>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {logsFor !== null && (
        <DashboardModal
          title={`${t('plugins.docker.logs')} · ${logsFor.pod}`}
          onClose={() => setLogsFor(null)}
          wide
        >
          {logsOutput === null ? (
            <CenterNotice text={t('plugins.kube.loading')} loading />
          ) : (
            <pre className="whitespace-pre-wrap break-words rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
              {logsOutput}
            </pre>
          )}
        </DashboardModal>
      )}
    </div>
  );
}
