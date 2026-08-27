import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { GitBranch } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { parseLines } from './parse';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, ErrorBanner } from './ui';

type GitTab = 'status' | 'branches' | 'history';

interface GitBranchRow {
  name: string;
  current: boolean;
  sha: string;
  message: string;
}

interface GitStatusRow {
  state: string;
  path: string;
}

const STATE_TONES: Record<string, string> = {
  M: 'text-tokyo-yellow',
  A: 'text-tokyo-green',
  D: 'text-tokyo-red',
  '??': 'text-tokyo-cyan',
};

export function GitDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<GitTab>('status');
  const [branchLine, setBranchLine] = useState<string | null>(null);
  const [statusRows, setStatusRows] = useState<GitStatusRow[] | null>(null);
  const [branches, setBranches] = useState<GitBranchRow[] | null>(null);
  const [history, setHistory] = useState<Array<{ sha: string; message: string }> | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (nextTab: GitTab) => {
    clearError();
    setLoading(true);
    if (nextTab === 'status') {
      setBranchLine(null);
      setStatusRows(null);
      const result = await run('status');
      if (result) {
        const lines = parseLines(result.output);
        setBranchLine(lines[0]?.replace(/^##\s*/, '') ?? null);
        setStatusRows(
          lines.slice(1).map((line) => {
            const state = line.slice(0, 2).trim();
            return { state: state || 'M', path: line.slice(3).trim() || line.trim() };
          })
        );
      }
    } else if (nextTab === 'branches') {
      setBranches(null);
      const result = await run('branches');
      if (result) {
        setBranches(
          parseLines(result.output).map((line) => {
            const current = line.startsWith('*');
            const rest = current ? line.slice(1).trim() : line.trim();
            const fields = rest.split(/\s+/);
            const name = fields[0] ?? '';
            const sha = fields[1] ?? '';
            const message = fields.slice(2).join(' ');
            return { name, current, sha, message };
          })
        );
      }
    } else {
      setHistory(null);
      const result = await run('history');
      if (result) {
        setHistory(
          parseLines(result.output).map((line) => {
            const space = line.indexOf(' ');
            return space === -1
              ? { sha: line, message: '' }
              : { sha: line.slice(0, space), message: line.slice(space + 1) };
          })
        );
      }
    }
    setLoading(false);
  }, [clearError, run]);

  useEffect(() => {
    load(tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<GitBranch className="h-4 w-4 text-tokyo-orange" />}
        title={t('plugins.git.title')}
        tabs={[
          { id: 'status', label: t('plugins.git.status') },
          { id: 'branches', label: t('plugins.git.branches') },
          { id: 'history', label: t('plugins.git.history') },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as GitTab)}
        onRefresh={() => load(tab)}
        refreshing={loading}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === 'status' ? (
          loading && statusRows === null ? (
            <CenterNotice text={t('plugins.git.loading')} loading />
          ) : (
            <div className="p-2">
              {branchLine && (
                <div className="mb-2 inline-flex items-center gap-1.5 rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark px-2.5 py-1 font-mono text-xs text-tokyo-cyan">
                  <GitBranch className="h-3.5 w-3.5" />
                  {branchLine}
                </div>
              )}
              {(statusRows ?? []).length === 0 ? (
                <CenterNotice text={t('plugins.git.clean')} />
              ) : (
                <div className="overflow-hidden rounded-lg border border-tokyo-bg-hl">
                  <table className="w-full border-separate border-spacing-0 text-left text-xs">
                    <tbody className="font-mono text-tokyo-fg">
                      {(statusRows ?? []).map((row, index) => (
                        <tr key={`${row.path}-${index}`} className="hover:bg-tokyo-bg-hl/40">
                          <td className="w-16 border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                            <span className={cn('font-semibold', STATE_TONES[row.state] ?? 'text-tokyo-comment')}>
                              {row.state}
                            </span>
                          </td>
                          <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                            <span className="block truncate" title={row.path}>{row.path}</span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )
        ) : tab === 'branches' ? (
          loading && branches === null ? (
            <CenterNotice text={t('plugins.git.loading')} loading />
          ) : (branches ?? []).length === 0 ? (
            <CenterNotice text={t('common.noData')} />
          ) : (
            <div className="m-2 overflow-hidden rounded-lg border border-tokyo-bg-hl">
              <table className="w-full border-separate border-spacing-0 text-left text-xs">
                <tbody className="font-mono text-tokyo-fg">
                  {(branches ?? []).map((branch) => (
                    <tr key={branch.name} className="hover:bg-tokyo-bg-hl/40">
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className={cn('flex items-center gap-1.5', branch.current ? 'text-tokyo-cyan' : '')}>
                          {branch.current && <span className="text-[10px]">●</span>}
                          {branch.name}
                        </span>
                      </td>
                      <td className="w-20 border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-tokyo-comment">{branch.sha}</td>
                      <td className="max-w-[280px] border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className="block truncate text-tokyo-comment" title={branch.message}>
                          {branch.message || '-'}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        ) : loading && history === null ? (
          <CenterNotice text={t('plugins.git.loading')} loading />
        ) : (history ?? []).length === 0 ? (
          <CenterNotice text={t('common.noData')} />
        ) : (
          <div className="m-2 space-y-0.5">
            {(history ?? []).map((commit, index) => (
              <div
                key={`${commit.sha}-${index}`}
                className="flex items-baseline gap-3 rounded-md px-2 py-1 font-mono text-xs hover:bg-tokyo-bg-hl/40"
              >
                <span className="flex-shrink-0 text-tokyo-orange">{commit.sha}</span>
                <span className="min-w-0 flex-1 truncate text-tokyo-fg" title={commit.message}>
                  {commit.message}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
