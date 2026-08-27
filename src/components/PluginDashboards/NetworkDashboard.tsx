import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Network } from 'lucide-react';
import type { PluginRecord } from '../../plugins/types';
import { usePluginAction } from './usePluginAction';
import { CenterNotice, DashboardHeader, ErrorBanner } from './ui';

type NetworkTab = 'sockets' | 'routes' | 'addresses' | 'dns';

interface ListeningSocket {
  protocol: string;
  local: string;
  peer: string;
  process: string;
}

function parseSockets(output: string): ListeningSocket[] {
  return output
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('State'))
    .map((line) => {
      const fields = line.split(/\s+/);
      const rest = fields.slice(5).join(' ');
      const processMatch = rest.match(/"([^"]+)",pid=(\d+)/);
      return {
        protocol: (fields[0] ?? '').replace(/6$/, ''),
        local: fields[4] ?? '',
        peer: fields[5]?.startsWith('users:') ? '' : fields[5] ?? '',
        process: processMatch ? `${processMatch[1]} (${processMatch[2]})` : '',
      };
    })
    .filter((socket) => socket.local.length > 0);
}

export function NetworkDashboard({ plugin, sessionId }: { plugin: PluginRecord; sessionId: string }) {
  const { t } = useTranslation();
  const { run, error, clearError } = usePluginAction(plugin.manifest.id, sessionId);
  const [tab, setTab] = useState<NetworkTab>('sockets');
  const [sockets, setSockets] = useState<ListeningSocket[] | null>(null);
  const [textOutput, setTextOutput] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async (nextTab: NetworkTab) => {
    clearError();
    setLoading(true);
    setSockets(null);
    setTextOutput(null);
    if (nextTab === 'sockets') {
      const result = await run('sockets');
      if (result) setSockets(parseSockets(result.output));
    } else {
      const actionId = nextTab === 'routes' ? 'routes' : nextTab === 'addresses' ? 'addresses' : 'dns';
      const result = await run(actionId);
      if (result) setTextOutput(result.output || t('common.noData'));
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
        icon={<Network className="h-4 w-4 text-tokyo-cyan" />}
        title={t('plugins.network.title')}
        tabs={[
          { id: 'sockets', label: t('plugins.network.sockets') },
          { id: 'routes', label: t('plugins.network.routes') },
          { id: 'addresses', label: t('plugins.network.addresses') },
          { id: 'dns', label: 'DNS' },
        ]}
        activeTab={tab}
        onTabChange={(next) => setTab(next as NetworkTab)}
        onRefresh={() => load(tab)}
        refreshing={loading}
      />
      <ErrorBanner message={error} onDismiss={clearError} />

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === 'sockets' ? (
          loading && sockets === null ? (
            <CenterNotice text={t('plugins.network.loading')} loading />
          ) : (sockets ?? []).length === 0 ? (
            <CenterNotice text={t('common.noData')} />
          ) : (
            <div className="m-2 overflow-hidden rounded-lg border border-tokyo-bg-hl">
              <table className="w-full border-separate border-spacing-0 text-left text-xs">
                <thead className="bg-tokyo-bg-dark text-tokyo-comment">
                  <tr>
                    <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.network.proto')}</th>
                    <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.network.localAddress')}</th>
                    <th className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">{t('plugins.network.process')}</th>
                  </tr>
                </thead>
                <tbody className="font-mono text-tokyo-fg">
                  {(sockets ?? []).map((socket, index) => (
                    <tr key={`${socket.protocol}-${socket.local}-${index}`} className="hover:bg-tokyo-bg-hl/40">
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        <span className="rounded border border-tokyo-bg-hl px-1.5 py-0.5 text-[10px] uppercase text-tokyo-cyan">
                          {socket.protocol}
                        </span>
                      </td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">{socket.local}</td>
                      <td className="border-b border-tokyo-bg-hl/60 px-3 py-1.5">
                        {socket.process || <span className="text-tokyo-comment">-</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
        ) : loading && textOutput === null ? (
          <CenterNotice text={t('plugins.network.loading')} loading />
        ) : (
          <pre className="m-2 overflow-auto rounded-lg border border-tokyo-bg-hl bg-tokyo-bg p-3 font-mono text-xs leading-5 text-tokyo-fg">
            {textOutput ?? t('common.noData')}
          </pre>
        )}
      </div>
    </div>
  );
}
