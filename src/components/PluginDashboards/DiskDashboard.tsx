import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { HardDrive } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { parseDelimited } from './parse';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, DataGrid, ErrorBanner } from './ui';

type DiskTab = 'filesystems' | 'directories';

interface FilesystemRow {
  filesystem: string;
  size: string;
  used: string;
  available: string;
  percent: number;
  mount: string;
}

function usageTone(percent: number) {
  if (percent >= 90) return 'bg-tokyo-red';
  if (percent >= 70) return 'bg-tokyo-yellow';
  return 'bg-tokyo-green';
}

function usageTextTone(percent: number) {
  if (percent >= 90) return 'text-tokyo-red';
  if (percent >= 70) return 'text-tokyo-yellow';
  return 'text-tokyo-green';
}

export function DiskDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<DiskTab>('filesystems');
  const [filesystems, setFilesystems] = useState<FilesystemRow[] | null>(null);
  const [directories, setDirectories] = useState<{ columns: string[]; rows: string[][] } | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (nextTab: DiskTab) => {
    clearError();
    setLoading(true);
    if (nextTab === 'filesystems') {
      setFilesystems(null);
      const result = await run('filesystems');
      if (result) {
        setFilesystems(
          result.output
            .split('\n')
            .map((line) => line.trim())
            .filter((line) => line.length > 0)
            .map((line) => line.split(/\s+/))
            .filter((fields) => fields.length >= 6)
            .map((fields) => ({
              filesystem: fields[0],
              size: fields[1],
              used: fields[2],
              available: fields[3],
              percent: Number.parseInt((fields[4] ?? '0%').replace('%', ''), 10) || 0,
              mount: fields[5],
            }))
            .filter((row) => !row.filesystem.startsWith('Filesystem'))
        );
      }
    } else {
      setDirectories(null);
      const result = await run('current-directory');
      if (result) {
        setDirectories({
          columns: [t('plugins.disk.directory'), t('plugins.disk.size')],
          rows: parseDelimited(result.output, '\t').map((fields) => [fields[0] ?? '', fields[1] ?? '']),
        });
      }
    }
    setLoading(false);
  }, [clearError, run, t]);

  useEffect(() => {
    load(tab);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<HardDrive className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.disk.title')}
        tabs={[
          { id: 'filesystems', label: t('plugins.disk.filesystems') },
          { id: 'directories', label: t('plugins.disk.directories') },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as DiskTab)}
        onRefresh={() => load(tab)}
        refreshing={loading}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === 'filesystems' ? (
          loading && filesystems === null ? (
            <CenterNotice text={t('plugins.disk.loading')} loading />
          ) : (
            <div className="space-y-2 p-2">
              {(filesystems ?? []).map((row) => (
                <div key={`${row.filesystem}-${row.mount}`} className="rounded-lg border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2.5">
                  <div className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-xs text-tokyo-fg" title={row.filesystem}>
                      {row.mount}
                    </span>
                    <span className="text-[10px] text-tokyo-comment">{row.filesystem}</span>
                    <span className={cn('text-xs font-semibold tabular-nums', usageTextTone(row.percent))}>
                      {row.percent}%
                    </span>
                  </div>
                  <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-tokyo-bg-hl">
                    <div
                      className={cn('h-full rounded-full transition-all', usageTone(row.percent))}
                      style={{ width: `${Math.min(row.percent, 100)}%` }}
                    />
                  </div>
                  <div className="mt-1 flex justify-between text-[10px] tabular-nums text-tokyo-comment">
                    <span>{t('plugins.disk.used', { used: row.used, size: row.size })}</span>
                    <span>{t('plugins.disk.free', { available: row.available })}</span>
                  </div>
                </div>
              ))}
            </div>
          )
        ) : loading && directories === null ? (
          <CenterNotice text={t('plugins.disk.loading')} loading />
        ) : (
          <DataGrid
            columns={directories?.columns ?? []}
            rows={directories?.rows ?? []}
            emptyText={t('common.noData')}
          />
        )}
      </div>
    </div>
  );
}
