import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Bot, CheckCircle2, Circle, X, XCircle } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useAgentActivityStore, type AgentActivity } from '../../stores/agentActivityStore';
import { useSessionStore } from '../../stores/sessionStore';

interface AgentActivityPanelProps {
  open: boolean;
  onClose: () => void;
  onSessionsChanged: () => void;
}

function ActivityIcon({ status }: { status: AgentActivity['status'] }) {
  if (status === 'succeeded') return <CheckCircle2 className="h-3.5 w-3.5 text-tokyo-green" aria-hidden="true" />;
  if (status === 'failed') return <XCircle className="h-3.5 w-3.5 text-tokyo-red" aria-hidden="true" />;
  return <Circle className="h-3.5 w-3.5 fill-tokyo-blue text-tokyo-blue" aria-hidden="true" />;
}

function sessionFor(activity: AgentActivity) {
  return useSessionStore.getState().sessions.find((session) => activity.sessionId
    && (session.id === activity.sessionId || session.id.startsWith(activity.sessionId)));
}

/** Visible even when the full history is closed; never injects fake bytes into the human PTY. */
export function AgentActivityNotice({ onOpen }: { onOpen: () => void }) {
  const activity = useAgentActivityStore((state) => state.activities[0]);
  const error = useAgentActivityStore((state) => state.error);
  if (!activity && !error) return null;
  return <button type="button" onClick={onOpen} aria-label="Open Agent command history"
    className="flex min-h-8 w-full items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-3 py-1 text-left text-xs text-tokyo-fg">
    <Bot className="h-4 w-4 shrink-0 text-tokyo-blue" />
    <span className="shrink-0">Agent / CLI</span>
    {activity && <ActivityIcon status={activity.status} />}
    <span className="min-w-0 flex-1 truncate font-mono">{error ?? activity?.summary}</span>
    <span className="shrink-0 text-tokyo-comment">History</span>
  </button>;
}

export function AgentActivityPanel({ open, onClose, onSessionsChanged }: AgentActivityPanelProps) {
  const { t } = useTranslation();
  const { activities, hasOlder, loadingOlder, error, refresh, loadOlder } = useAgentActivityStore();
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;
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
    // Push notifications reduce latency for GUI-owned MCP sessions; the durable
    // cursor remains authoritative and catches CLI/daemon activity and missed events.
    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      if (stopped) return;
      const stop = await listen('agent-gateway-activity', () => { void update(); });
      if (stopped) stop(); else unlisten = stop;
    }).catch(() => { /* Browser-only preview has no native event bridge. */ });
    return () => { stopped = true; clearTimeout(timer); unlisten?.(); document.removeEventListener('visibilitychange', resume); };
  }, [onSessionsChanged, refresh]);

  return <aside className={cn('agent-activity-panel min-h-0 w-80 flex-shrink-0 flex-col border-l border-tokyo-bg-hl bg-tokyo-bg-dark', open ? 'flex' : 'hidden')}
    aria-label={t('agentActivity.title')}>
    <header className="flex h-10 shrink-0 items-center gap-2 border-b border-tokyo-bg-hl px-3">
      <Bot className="h-4 w-4 text-tokyo-blue" />
      <h2 className="min-w-0 flex-1 truncate text-sm font-medium text-tokyo-fg">{t('agentActivity.title')}</h2>
      <button className="icon-button h-7 w-7" onClick={onClose} aria-label={t('agentActivity.close')}><X className="h-3.5 w-3.5" /></button>
    </header>
    {error && <p role="alert" className="px-3 py-2 text-xs text-tokyo-red">{error}</p>}
    <p className="border-b border-tokyo-bg-hl px-3 py-2 text-xs text-tokyo-comment">Command history is local and encrypted. Input “succeeded” means sent to the terminal, not a verified command exit status.</p>
    <ol className="min-h-0 flex-1 overflow-y-auto">
      {!activities.length && <li className="px-3 py-6 text-sm text-tokyo-comment">{t('agentActivity.empty')}</li>}
      {activities.map((activity) => {
        const session = sessionFor(activity);
        return <li key={activity.id} className="border-b border-tokyo-bg-hl px-3 py-2.5">
          <div className="flex items-center gap-2">
            <ActivityIcon status={activity.status} />
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-tokyo-fg">{activity.tool}</span>
            <time className="text-xs text-tokyo-comment" dateTime={new Date(activity.timestamp).toISOString()}>{new Date(activity.timestamp).toLocaleTimeString()}</time>
          </div>
          <pre className="mt-1 whitespace-pre-wrap break-all font-mono text-xs leading-5 text-tokyo-fg">{activity.summary}</pre>
          <div className="mt-1 flex items-center gap-2 text-xs text-tokyo-comment">
            <span>{activity.status}</span>
            {session ? <button type="button" className="text-tokyo-blue underline" onClick={() => useSessionStore.getState().setActiveSession(session.id)}>{session.serverName}</button>
              : activity.sessionId && <span>Session {activity.sessionId.slice(0, 8)}</span>}
          </div>
        </li>;
      })}
      {hasOlder && <li><button type="button" className="w-full px-3 py-3 text-sm text-tokyo-blue" disabled={loadingOlder} onClick={() => { void loadOlder(); }}>{loadingOlder ? 'Loading…' : 'Load earlier commands'}</button></li>}
    </ol>
  </aside>;
}
