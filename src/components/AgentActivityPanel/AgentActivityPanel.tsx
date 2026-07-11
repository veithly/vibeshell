import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Bot, CheckCircle2, Circle, X, XCircle } from 'lucide-react';
import { cn } from '../../lib/utils';

type ActivityStatus = 'started' | 'succeeded' | 'failed';

interface AgentActivityEvent {
  id: string;
  tool: string;
  summary: string;
  status: ActivityStatus;
  sessionId?: string;
  timestamp: number;
}

interface AgentActivityPanelProps {
  open: boolean;
  onClose: () => void;
  onSessionsChanged: () => void;
}

function formatTime(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(timestamp));
}

function ActivityIcon({ status }: { status: ActivityStatus }) {
  if (status === 'succeeded') {
    return <CheckCircle2 className="h-3.5 w-3.5 text-tokyo-green" aria-hidden="true" />;
  }
  if (status === 'failed') {
    return <XCircle className="h-3.5 w-3.5 text-tokyo-red" aria-hidden="true" />;
  }
  return <Circle className="h-3.5 w-3.5 fill-tokyo-blue text-tokyo-blue" aria-hidden="true" />;
}

export function AgentActivityPanel({ open, onClose, onSessionsChanged }: AgentActivityPanelProps) {
  const { t } = useTranslation();
  const [activities, setActivities] = useState<AgentActivityEvent[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const subscribe = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<AgentActivityEvent>('agent-gateway-activity', (event) => {
          const activity = event.payload;
          setActivities((current) => {
            const existingIndex = current.findIndex((item) => item.id === activity.id);
            if (existingIndex === -1) {
              return [activity, ...current].slice(0, 100);
            }
            return current.map((item, index) => index === existingIndex ? activity : item);
          });

          if (
            activity.status === 'succeeded'
            && (activity.tool === 'session_create' || activity.tool === 'session_kill')
          ) {
            onSessionsChanged();
          }
        });
      } catch {
        // Browser-only previews do not have Tauri's event bridge.
      }
    };

    void subscribe();
    return () => unlisten?.();
  }, [onSessionsChanged]);

  return (
    <aside
      className={cn(
        'min-h-0 w-80 flex-shrink-0 flex-col border-l border-tokyo-bg-hl bg-tokyo-bg-dark',
        open ? 'flex' : 'hidden'
      )}
      aria-label={t('agentActivity.title')}
    >
      <header className="flex h-10 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl px-3">
        <Bot className="h-4 w-4 text-tokyo-blue" aria-hidden="true" />
        <h2 className="min-w-0 flex-1 truncate text-sm font-medium text-tokyo-fg">
          {t('agentActivity.title')}
        </h2>
        <span className="inline-flex items-center gap-1 text-[11px] text-tokyo-green">
          <span className="h-1.5 w-1.5 rounded-full bg-tokyo-green" />
          {t('agentActivity.connected')}
        </span>
        <button
          className="icon-button h-7 w-7"
          onClick={onClose}
          aria-label={t('agentActivity.close')}
          title={t('agentActivity.close')}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </header>

      {activities.length === 0 ? (
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
          <Bot className="mb-3 h-7 w-7 text-tokyo-comment" aria-hidden="true" />
          <p className="text-sm font-medium text-tokyo-fg">{t('agentActivity.empty')}</p>
          <p className="mt-1 text-xs text-tokyo-comment">{t('agentActivity.ready')}</p>
        </div>
      ) : (
        <ol className="min-h-0 flex-1 overflow-y-auto" aria-live="polite">
          {activities.map((activity) => (
            <li key={activity.id} className="border-b border-tokyo-bg-hl px-3 py-2.5">
              <div className="flex items-start gap-2">
                <span className="mt-0.5 flex h-5 w-5 flex-shrink-0 items-center justify-center">
                  <ActivityIcon status={activity.status} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 flex-1 truncate font-mono text-[11px] font-medium text-tokyo-fg">
                      {activity.tool}
                    </span>
                    <time className="flex-shrink-0 text-[10px] text-tokyo-comment" dateTime={new Date(activity.timestamp).toISOString()}>
                      {formatTime(activity.timestamp)}
                    </time>
                  </div>
                  <p className="mt-1 break-words text-xs leading-5 text-tokyo-comment">
                    {activity.summary}
                  </p>
                </div>
              </div>
            </li>
          ))}
        </ol>
      )}
    </aside>
  );
}
