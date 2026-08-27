import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { KeyRound, RefreshCw, Search, Trash2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { PluginRecord } from '../../plugins/types';
import { parseLines, parseRedisInfo, parseRedisKeyspace } from './parse';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, ErrorBanner, StatCard } from './ui';

interface KeyDetail {
  key: string;
  type: string | null;
  ttl: string | null;
  value: string | null;
  loading: boolean;
}

export function RedisConsole({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, runningAction, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [pingOk, setPingOk] = useState<boolean | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [uptimeDays, setUptimeDays] = useState<string | null>(null);
  const [memory, setMemory] = useState<string | null>(null);
  const [clients, setClients] = useState<string | null>(null);
  const [totalKeys, setTotalKeys] = useState<number | null>(null);
  const [pattern, setPattern] = useState('*');
  const [keys, setKeys] = useState<string[]>([]);
  const [scanning, setScanning] = useState(false);
  const [detail, setDetail] = useState<KeyDetail | null>(null);

  const loadOverview = useCallback(async () => {
    clearError();
    const ping = await run('ping');
    setPingOk(ping !== null && ping.output.trim().toUpperCase() === 'PONG');

    const server = await run('info', { section: 'server' });
    if (server) {
      const info = parseRedisInfo(server.output);
      setVersion(info.redis_version ?? null);
      setUptimeDays(info.uptime_in_days ?? null);
    }
    const memoryInfo = await run('info', { section: 'memory' });
    if (memoryInfo) {
      setMemory(parseRedisInfo(memoryInfo.output).used_memory_human ?? null);
    }
    const clientsInfo = await run('info', { section: 'clients' });
    if (clientsInfo) {
      setClients(parseRedisInfo(clientsInfo.output).connected_clients ?? null);
    }
    const keyspace = await run('keyspace');
    if (keyspace) {
      setTotalKeys(parseRedisKeyspace(keyspace.output));
    }
  }, [clearError, run]);

  const scan = useCallback(async (nextPattern: string) => {
    setScanning(true);
    setDetail(null);
    const outcome = await run('scan-keys', { pattern: nextPattern });
    if (outcome) {
      // --scan streams keys; keep the first 500 for the browser.
      setKeys(parseLines(outcome.output).slice(0, 500));
    }
    setScanning(false);
  }, [run]);

  useEffect(() => {
    void loadOverview();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const openKey = async (key: string) => {
    setDetail({ key, type: null, ttl: null, value: null, loading: true });
    const typeResult = await run('key-type', { key });
    const ttlResult = await run('key-ttl', { key });
    const type = typeResult?.output.trim() ?? null;
    let value: string | null = null;
    if (type === 'string') {
      const valueResult = await run('get-key', { key });
      value = valueResult?.output ?? null;
    }
    setDetail({
      key,
      type,
      ttl: ttlResult?.output.trim() ?? null,
      value,
      loading: false,
    });
  };

  const deleteKey = async (key: string) => {
    if (!window.confirm(t('plugins.actionConfirm', { name: `DEL ${key}` }))) return;
    await run('delete-key', { key });
    setDetail(null);
    void scan(pattern);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <DashboardHeader
        icon={<KeyRound className="h-4 w-4 text-tokyo-red" />}
        title="Redis"
        badge={version ? `v${version}` : null}
        onRefresh={() => void loadOverview()}
        refreshing={runningAction !== null}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="flex flex-wrap gap-2 px-3 py-2">
        <StatCard
          label={t('plugins.redis.status')}
          value={pingOk === null ? '…' : pingOk ? 'PONG' : t('plugins.redis.down')}
          tone={pingOk ? 'good' : 'bad'}
        />
        <StatCard label={t('plugins.redis.keys')} value={totalKeys ?? '…'} />
        <StatCard label={t('plugins.redis.memory')} value={memory ?? '…'} />
        <StatCard label={t('plugins.redis.clients')} value={clients ?? '…'} />
        {uptimeDays !== null && (
          <StatCard label={t('plugins.redis.uptimeDays')} value={uptimeDays} />
        )}
      </div>

      <div className="flex items-center gap-2 border-y border-tokyo-bg-hl px-3 py-2">
        <Search className="h-3.5 w-3.5 flex-shrink-0 text-tokyo-comment" />
        <input
          className="h-8 min-w-0 flex-1 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 font-mono text-xs text-tokyo-fg outline-none focus:border-tokyo-cyan"
          placeholder="user:*"
          value={pattern}
          onChange={(event) => setPattern(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') void scan(pattern);
          }}
        />
        <button
          className="flex h-8 flex-shrink-0 items-center gap-1.5 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
          onClick={() => void scan(pattern)}
          disabled={scanning}
        >
          <RefreshCw className={cn('h-3.5 w-3.5', scanning && 'animate-spin')} />
          {t('plugins.redis.scan')}
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="min-w-0 flex-1 overflow-y-auto">
          {scanning && keys.length === 0 ? (
            <CenterNotice text={t('plugins.redis.scanning')} loading />
          ) : keys.length === 0 ? (
            <CenterNotice text={t('plugins.redis.scanHint')} />
          ) : (
            <table className="w-full border-separate border-spacing-0 text-left text-xs">
              <tbody className="font-mono text-tokyo-fg">
                {keys.map((key) => (
                  <tr key={key} className="hover:bg-tokyo-bg-hl/40">
                    <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                      <button
                        className={cn(
                          'block max-w-full truncate hover:text-tokyo-cyan hover:underline',
                          detail?.key === key && 'text-tokyo-cyan'
                        )}
                        onClick={() => void openKey(key)}
                      >
                        {key}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {detail && (
          <aside className="w-72 flex-shrink-0 overflow-y-auto border-l border-tokyo-bg-hl p-3">
            <div className="mb-2 break-all font-mono text-xs font-semibold text-tokyo-fg">{detail.key}</div>
            {detail.loading ? (
              <CenterNotice text={t('plugins.redis.loadingKey')} loading />
            ) : (
              <>
                <div className="space-y-1 text-xs">
                  <div className="flex gap-2">
                    <span className="w-12 flex-shrink-0 text-tokyo-comment">{t('plugins.redis.type')}</span>
                    <span className="font-mono text-tokyo-fg">{detail.type ?? '-'}</span>
                  </div>
                  <div className="flex gap-2">
                    <span className="w-12 flex-shrink-0 text-tokyo-comment">TTL</span>
                    <span className="font-mono text-tokyo-fg">
                      {detail.ttl === '-1' ? t('plugins.redis.forever') : detail.ttl ?? '-'}
                    </span>
                  </div>
                </div>
                <div className="mt-2 text-[10px] uppercase tracking-wide text-tokyo-comment">
                  {t('plugins.redis.value')}
                </div>
                <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-md border border-tokyo-bg-hl bg-tokyo-bg p-2 font-mono text-xs text-tokyo-fg">
                  {detail.value ?? (detail.type && detail.type !== 'string'
                    ? t('plugins.redis.nonStringValue', { type: detail.type })
                    : t('common.noData'))}
                </pre>
                <button
                  className="mt-3 flex h-8 w-full items-center justify-center gap-1.5 rounded-md border border-tokyo-red/50 bg-tokyo-red/10 text-xs font-medium text-tokyo-red hover:bg-tokyo-red/20"
                  onClick={() => void deleteKey(detail.key)}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t('plugins.redis.deleteKey')}
                </button>
              </>
            )}
          </aside>
        )}
      </div>
    </div>
  );
}
