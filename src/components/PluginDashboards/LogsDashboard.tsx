import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { FileText, RefreshCw, Search } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { usePluginAction } from './usePluginAction';
import { DashboardHeader, ErrorBanner } from './ui';

const AUTO_OPTIONS = [
  { value: 0, labelKey: 'plugins.docker.refreshOff' },
  { value: 5, labelKey: 'plugins.docker.refresh10s' },
  { value: 15, labelKey: 'plugins.docker.refresh30s' },
  { value: 30, labelKey: 'plugins.docker.refresh60s' },
];

function lineTone(line: string): string {
  const lowered = line.toLowerCase();
  if (lowered.includes('crit') || lowered.includes('emerg') || lowered.includes('failed to')) {
    return 'text-tokyo-red';
  }
  if (lowered.includes('error')) {
    return 'text-tokyo-red';
  }
  if (lowered.includes('warn')) {
    return 'text-tokyo-yellow';
  }
  return 'text-tokyo-fg';
}

/**
 * Journal viewer: pick a systemd unit (or read the global journal), tail it
 * with optional auto-refresh, and filter/highlight lines client-side.
 */
export function LogsDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [service, setService] = useState('');
  const [lines, setLines] = useState<string[] | null>(null);
  const [filter, setFilter] = useState('');
  const [autoSeconds, setAutoSeconds] = useState(0);
  const bodyRef = useRef<HTMLDivElement>(null);

  const load = useCallback(async () => {
    clearError();
    const trimmed = service.trim();
    const result = trimmed
      ? await run('service', { service: trimmed })
      : await run('recent');
    if (result) {
      setLines(result.output.split('\n').filter((line) => line.length > 0));
    }
  }, [clearError, run, service]);

  useEffect(() => {
    setLines(null);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (autoSeconds === 0) return;
    const interval = window.setInterval(() => {
      void load();
    }, autoSeconds * 1000);
    return () => window.clearInterval(interval);
  }, [autoSeconds, load]);

  const query = filter.trim().toLowerCase();
  const visibleLines = useMemo(() => {
    if (lines === null) return null;
    return query === '' ? lines : lines.filter((line) => line.toLowerCase().includes(query));
  }, [lines, query]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<FileText className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.logs.title')}
        onRefresh={() => void load()}
        refreshing={runningAction !== null}
        extra={
          <>
            <input
              className="h-7 w-44 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 font-mono text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
              placeholder={t('plugins.logs.servicePlaceholder')}
              value={service}
              onChange={(event) => setService(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  setLines(null);
                  void load();
                }
              }}
            />
            <select
              className="mr-1 h-7 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-xs text-tokyo-fg outline-none"
              value={autoSeconds}
              onChange={(event) => setAutoSeconds(Number(event.target.value))}
              aria-label={t('plugins.docker.autoRefresh')}
            >
              {AUTO_OPTIONS.map((option) => (
                <option key={option.value} value={option.value}>
                  {t(option.labelKey)}
                </option>
              ))}
            </select>
          </>
        }
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="flex items-center gap-2 border-b border-tokyo-bg-hl px-3 py-1.5">
        <Search className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-comment" />
        <input
          className="h-7 min-w-0 flex-1 bg-transparent text-xs text-tokyo-fg outline-none placeholder:text-tokyo-comment"
          placeholder={t('plugins.logs.filterPlaceholder')}
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
        />
        {visibleLines !== null && (
          <span className="text-[10px] tabular-nums text-tokyo-comment">
            {visibleLines.length}/{lines?.length ?? 0}
          </span>
        )}
      </div>

      <div ref={bodyRef} className="min-h-0 flex-1 overflow-auto bg-tokyo-bg p-2">
        {visibleLines === null ? (
          <div className="flex h-full items-center justify-center gap-2 text-xs text-tokyo-comment">
            <RefreshCw className="h-4 w-4 animate-spin" />
            {t('plugins.logs.loading')}
          </div>
        ) : visibleLines.length === 0 ? (
          <div className="flex h-full items-center justify-center text-xs text-tokyo-comment">
            {t('common.noData')}
          </div>
        ) : (
          <div className="rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark/40 p-2 font-mono text-[11px] leading-5">
            {visibleLines.map((line, index) => (
              <div key={index} className={cn('whitespace-pre-wrap break-all', lineTone(line))}>
                {line}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
