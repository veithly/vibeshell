import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Clock, Plus, Trash2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { parseLines } from './parse';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, ErrorBanner } from './ui';

type CronTab = 'crontab' | 'timers' | 'cron-d' | 'crontab-file';

interface CronEntry {
  line: string;
  schedule: string;
  command: string;
}

function parseCrontab(output: string): CronEntry[] {
  return parseLines(output)
    .filter((line) => !line.startsWith('#'))
    .map((line) => {
      const fields = line.trim().split(/\s+/);
      return {
        line: line.trim(),
        schedule: fields.slice(0, 5).join(' '),
        command: fields.slice(5).join(' '),
      };
    })
    .filter((entry) => entry.command.length > 0);
}

export function CronDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<CronTab>('crontab');
  const [entries, setEntries] = useState<CronEntry[] | null>(null);
  const [textOutput, setTextOutput] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [newSchedule, setNewSchedule] = useState('');
  const [newCommand, setNewCommand] = useState('');
  const [search, setSearch] = useState('');

  const load = useCallback(async (nextTab: CronTab) => {
    clearError();
    setLoading(true);
    setEntries(null);
    setTextOutput(null);
    if (nextTab === 'crontab') {
      const result = await run('crontab-list');
      if (result) setEntries(parseCrontab(result.output));
    } else if (nextTab === 'timers') {
      const result = await run('timers');
      if (result) setTextOutput(result.output || t('common.noData'));
    } else if (nextTab === 'cron-d') {
      const result = await run('cron-d');
      if (result) setTextOutput(result.output || t('common.noData'));
    } else {
      const result = await run('crontab-file');
      if (result) setTextOutput(result.output || t('common.noData'));
    }
    setLoading(false);
  }, [clearError, run, t]);

  useEffect(() => {
    load(tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  const addEntry = async () => {
    const schedule = newSchedule.trim();
    const command = newCommand.trim();
    if (!schedule || !command) return;
    const line = `${schedule} ${command}`;
    if (!window.confirm(t('plugins.actionConfirm', { name: `+ ${line}` }))) return;
    const result = await run('cron-add', { line });
    if (result !== null) {
      setNewSchedule('');
      setNewCommand('');
      load('crontab');
    }
  };

  const removeEntry = async (line: string) => {
    if (!window.confirm(t('plugins.actionConfirm', { name: `- ${line}` }))) return;
    const result = await run('cron-remove', { line });
    if (result !== null) load('crontab');
  };

  const filtered = (entries ?? []).filter((entry) =>
    search.trim() === ''
    || entry.command.toLowerCase().includes(search.trim().toLowerCase())
    || entry.schedule.includes(search.trim())
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<Clock className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.cron.title')}
        tabs={[
          { id: 'crontab', label: t('plugins.cron.crontab') },
          { id: 'timers', label: t('plugins.cron.timers') },
          { id: 'cron-d', label: '/etc/cron.d' },
          { id: 'crontab-file', label: '/etc/crontab' },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as CronTab)}
        onRefresh={() => load(tab)}
        refreshing={loading}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === 'crontab' ? (
          <>
            <div className="flex flex-wrap items-center gap-2 p-2">
              <input
                className="h-8 w-36 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 font-mono text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                placeholder="*/5 * * * *"
                value={newSchedule}
                onChange={(event) => setNewSchedule(event.target.value)}
              />
              <input
                className="h-8 min-w-[160px] flex-1 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 font-mono text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                placeholder="/usr/local/bin/backup"
                value={newCommand}
                onChange={(event) => setNewCommand(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') void addEntry();
                }}
              />
              <button
                className="flex h-8 items-center gap-1.5 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
                onClick={() => void addEntry()}
                disabled={runningAction !== null || !newSchedule.trim() || !newCommand.trim()}
              >
                <Plus className="h-3.5 w-3.5" />
                {t('plugins.cron.add')}
              </button>
              <input
                className="ml-auto h-8 w-40 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
                placeholder={t('plugins.cron.search')}
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </div>
            {loading && entries === null ? (
              <CenterNotice text={t('plugins.cron.loading')} loading />
            ) : filtered.length === 0 ? (
              <CenterNotice text={t('plugins.cron.empty')} />
            ) : (
              <div className="m-2 overflow-hidden rounded-lg border border-tokyo-bg-hl">
                <table className="w-full border-separate border-spacing-0 text-left text-xs">
                  <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                    <tr>
                      <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.cron.schedule')}</th>
                      <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.cron.command')}</th>
                      <th className="border-b border-tokyo-bg-hl px-3 py-2 text-right font-medium">{t('plugins.docker.actions')}</th>
                    </tr>
                  </thead>
                  <tbody className="text-tokyo-fg">
                    {filtered.map((entry) => (
                      <tr key={entry.line} className="hover:bg-tokyo-bg-hl/40">
                        <td className="whitespace-nowrap border-b border-tokyo-bg-hl/60 px-3 py-1.5 font-mono text-tokyo-cyan">
                          {entry.schedule}
                        </td>
                        <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 font-mono">
                          <span className="block truncate" title={entry.command}>{entry.command}</span>
                        </td>
                        <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 text-right">
                          <button
                            className={cn('icon-button h-6 w-6 hover:text-tokyo-red')}
                            disabled={runningAction !== null}
                            onClick={() => void removeEntry(entry.line)}
                            title={t('plugins.cron.remove')}
                            aria-label={t('plugins.cron.remove')}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        ) : loading && textOutput === null ? (
          <CenterNotice text={t('plugins.cron.loading')} loading />
        ) : (
          <pre className="m-2 overflow-auto rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
            {textOutput ?? t('common.noData')}
          </pre>
        )}
      </div>
    </div>
  );
}
