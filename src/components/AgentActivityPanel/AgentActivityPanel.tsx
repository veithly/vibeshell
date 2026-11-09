import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { History, X } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useAgentActivityStore } from '../../stores/agentActivityStore';
import { useSessionStore } from '../../stores/sessionStore';

interface AgentActivityPanelProps {
  open: boolean;
  onSelectSession?: (id: string) => void;
  onClose: () => void;
  onSessionsChanged: () => void;
}

export function AgentActivityPanel({ open, onClose, onSessionsChanged, onSelectSession }: AgentActivityPanelProps) {
  const { t } = useTranslation();
  const { activities, hasOlder, loadingOlder, error, refresh, loadOlder } = useAgentActivityStore();
  const sessions = useSessionStore((state) => state.sessions);
  useEffect(() => {
    if (!open) return;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const update = async () => {
      const changed = await refresh();
      if (!stopped && changed) onSessionsChanged();
    };
    const poll = async () => {
      if (document.visibilityState !== 'hidden') await update();
      if (!stopped) timer = setTimeout(() => { void poll(); }, 1000);
    };
    const resume = () => { if (document.visibilityState !== 'hidden') void update(); };
    void poll();
    document.addEventListener('visibilitychange', resume);
    return () => { stopped = true; clearTimeout(timer); document.removeEventListener('visibilitychange', resume); };
  }, [open, onSessionsChanged, refresh]);

  if (!open) return null;
  return <aside className="agent-activity-panel flex min-h-0 w-80 max-w-full shrink-0 flex-col border-l border-tokyo-bg-hl bg-tokyo-bg-dark" aria-label={t('agentActivity.title')}>
    <header className="flex h-10 shrink-0 items-center gap-2 border-b border-tokyo-bg-hl px-3">
      <History className="h-4 w-4" aria-hidden="true" />
      <h2 className="flex-1 text-sm font-medium">{t('agentActivity.title')}</h2>
      <button className="icon-button h-7 w-7" onClick={onClose} aria-label={t('agentActivity.close')}><X className="h-3.5 w-3.5" /></button>
    </header>
    {error && <p role="alert" className="px-3 py-2 text-xs text-tokyo-red">{error}</p>}
    <ol className="min-h-0 flex-1 overflow-y-auto">
      {!activities.length && <li className="px-3 py-6 text-sm text-tokyo-comment">{t('agentActivity.empty')}</li>}
      {activities.map((row) => {
        const origin = row.origin === 'cli' || row.origin === 'user' ? row.origin : 'legacy';
        const color = origin === 'cli' ? 'text-tokyo-magenta' : origin === 'user' ? 'text-tokyo-cyan' : 'text-tokyo-comment';
        const session = sessions.find((s) => s.id === row.sessionId);
        const shortId = row.sessionId?.slice(0, 8);
        const operation = row.tool.replace(/^cli\./, '');
        return <li key={row.id} className="border-b border-tokyo-bg-hl px-3 py-2.5">
          <div className="flex items-baseline gap-2 text-xs">
            <span className={cn('font-medium', color)}>{t(`agentActivity.${origin}`)}</span>
            <time className="ml-auto text-tokyo-comment" title={new Date(row.timestamp).toLocaleString()} dateTime={new Date(row.timestamp).toISOString()}>{new Date(row.timestamp).toLocaleTimeString()}</time>
          </div>
          <pre className={cn('my-1 whitespace-pre-wrap break-all font-mono text-xs leading-5', color)}>{row.summary}</pre>
          <div className="flex flex-wrap items-baseline gap-2 text-xs text-tokyo-comment">
            {!['input', 'exec'].includes(operation) && <span>{t(`agentActivity.operations.${operation}`, { defaultValue: operation.replace(/_/g, ' ') })}</span>}
            {session ? <button type="button" title={session.id} className="text-tokyo-blue underline" onClick={() => (onSelectSession ?? useSessionStore.getState().setActiveSession)(session.id)}>{session.serverName} · {shortId}</button>
              : shortId && <span title={row.sessionId ?? undefined}>{shortId}</span>}
            {row.status !== 'succeeded' && <span className={row.status === 'failed' ? 'text-tokyo-red' : undefined}>{t(`agentActivity.${row.status}`)}</span>}
          </div>
        </li>;
      })}
      {hasOlder && <li><button type="button" className="w-full px-3 py-3 text-sm text-tokyo-blue" disabled={loadingOlder} onClick={() => { void loadOlder(); }}>{t(loadingOlder ? 'agentActivity.loading' : 'agentActivity.earlier')}</button></li>}
    </ol>
  </aside>;
}
