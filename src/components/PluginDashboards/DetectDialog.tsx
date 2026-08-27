import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, Radar } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSessionStore } from '../../stores/sessionStore';
import { useServerStore } from '../../stores/serverStore';
import {
  useDbConnectionsStore,
  type DbConnectionInput,
  type DatabaseSuggestion,
} from '../../stores/dbConnectionsStore';

interface DetectDialogProps {
  onClose: () => void;
  /** Called with prefilled connection fields for the add dialog. */
  onAdopt: (prefill: Partial<DbConnectionInput>) => void;
}

/**
 * "Detect from session": scans an SSH server for listening database ports and
 * database containers, then prefills the connection form with one click.
 */
export function DetectDialog({ onClose, onAdopt }: DetectDialogProps) {
  const { t } = useTranslation();
  const sessions = useSessionStore((state) => state.sessions);
  const servers = useServerStore((state) => state.servers);
  const detectFromSession = useDbConnectionsStore((state) => state.detectFromSession);

  const sshSessions = sessions.filter(
    (session) => session.sessionType === 'ssh' && session.state === 'connected'
  );
  const [sessionId, setSessionId] = useState(sshSessions[0]?.id ?? '');
  const [scanning, setScanning] = useState(false);
  const [suggestions, setSuggestions] = useState<DatabaseSuggestion[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!sessionId && sshSessions.length > 0) {
      setSessionId(sshSessions[0].id);
    }
  }, [sessionId, sshSessions]);

  const hostForSession = (id: string) => {
    const session = sessions.find((candidate) => candidate.id === id);
    return servers.find((server) => server.id === session?.serverId)?.host ?? '';
  };

  const scan = async () => {
    if (!sessionId) return;
    setScanning(true);
    setError(null);
    setSuggestions(null);
    const result = await detectFromSession(sessionId);
    if (result === null) {
      setError(t('plugins.dbConn.detectFailed'));
    } else {
      setSuggestions(result);
    }
    setScanning(false);
  };

  const adopt = (suggestion: DatabaseSuggestion) => {
    const host = hostForSession(sessionId);
    onAdopt({
      name: `${suggestion.engine} · ${host || suggestion.detail}`,
      engine: suggestion.engine === 'mysql' ? 'mysql' : 'postgresql',
      host,
      port: suggestion.port,
      username: suggestion.engine === 'mysql' ? 'root' : 'postgres',
      defaultDatabase: null,
    });
    onClose();
  };

  return (
    <div
      className="responsive-dialog-layer fixed inset-0 z-[120] flex items-center justify-center bg-tokyo-bg-dark/70 px-4"
      role="dialog"
      aria-modal="true"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="w-full max-w-md rounded-xl border border-tokyo-bg-hl bg-tokyo-bg-dark p-5 shadow-2xl">
        <h3 className="text-sm font-semibold text-tokyo-fg">{t('plugins.dbConn.detectTitle')}</h3>
        <p className="mt-1 text-xs text-tokyo-comment">{t('plugins.dbConn.detectHint')}</p>

        <div className="mt-4 flex items-center gap-2">
          <select
            className="h-9 min-w-0 flex-1 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2 text-sm text-tokyo-fg outline-none focus:border-tokyo-cyan"
            value={sessionId}
            onChange={(event) => setSessionId(event.target.value)}
          >
            {sshSessions.length === 0 && <option value="">{t('plugins.dbConn.noSessions')}</option>}
            {sshSessions.map((session) => (
              <option key={session.id} value={session.id}>
                {session.serverName} ({hostForSession(session.id)})
              </option>
            ))}
          </select>
          <button
            className="flex h-9 flex-shrink-0 items-center gap-1.5 rounded-md bg-tokyo-blue px-3 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
            onClick={() => void scan()}
            disabled={scanning || !sessionId}
          >
            {scanning ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Radar className="h-3.5 w-3.5" />}
            {t('plugins.dbConn.scan')}
          </button>
        </div>

        {error && (
          <div className="mt-3 rounded-md border border-tokyo-red/40 bg-tokyo-red/10 px-3 py-2 text-xs text-tokyo-red">
            {error}
          </div>
        )}

        {suggestions && suggestions.length === 0 && (
          <div className="mt-3 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-xs text-tokyo-comment">
            {t('plugins.dbConn.detectEmpty')}
          </div>
        )}

        {suggestions && suggestions.length > 0 && (
          <ul className="mt-3 space-y-1.5">
            {suggestions
              .filter((suggestion) => suggestion.engine === 'postgresql' || suggestion.engine === 'mysql')
              .map((suggestion) => (
              <li key={`${suggestion.source}-${suggestion.engine}-${suggestion.port}`}>
                <button
                  className={cn(
                    'flex w-full items-center gap-2 rounded-lg border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-left',
                    'transition-colors hover:border-tokyo-cyan'
                  )}
                  onClick={() => adopt(suggestion)}
                >
                  <span
                    className={cn(
                      'rounded border px-1.5 py-0.5 text-[10px] font-semibold uppercase',
                      suggestion.engine === 'postgresql'
                        ? 'border-tokyo-cyan/50 text-tokyo-cyan'
                        : suggestion.engine === 'mysql'
                          ? 'border-tokyo-orange/50 text-tokyo-orange'
                          : 'border-tokyo-red/50 text-tokyo-red'
                    )}
                  >
                    {suggestion.engine}
                  </span>
                  <span className="min-w-0 flex-1 truncate font-mono text-xs text-tokyo-fg">
                    :{suggestion.port} · {suggestion.detail}
                  </span>
                  <span className="text-[10px] text-tokyo-comment">
                    {t('plugins.dbConn.adopt')}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        {suggestions?.some((suggestion) => suggestion.engine === 'redis') && (
          <p className="mt-2 text-[10px] text-tokyo-comment">
            {t('plugins.dbConn.redisNote')}
          </p>
        )}

        <div className="mt-5 flex justify-end">
          <button
            className="h-8 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg hover:border-tokyo-comment"
            onClick={onClose}
          >
            {t('common.close')}
          </button>
        </div>
      </div>
    </div>
  );
}
