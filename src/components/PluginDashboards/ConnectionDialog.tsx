import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Loader2, PlugZap } from 'lucide-react';
import { cn } from '../../lib/utils';
import {
  useDbConnectionsStore,
  type DbConnection,
  type DbConnectionInput,
  type DbEngineId,
  type DbTestResult,
} from '../../stores/dbConnectionsStore';

interface ConnectionDialogProps {
  editing: DbConnection | null;
  prefill?: Partial<DbConnectionInput> | null;
  onClose: () => void;
  onSaved: (connection: DbConnection) => void;
}

const ENGINE_OPTIONS: Array<{ id: DbEngineId; label: string; port: number }> = [
  { id: 'postgresql', label: 'PostgreSQL', port: 5432 },
  { id: 'mysql', label: 'MySQL / MariaDB', port: 3306 },
];

export function ConnectionDialog({ editing, prefill, onClose, onSaved }: ConnectionDialogProps) {
  const { t } = useTranslation();
  const saveConnection = useDbConnectionsStore((state) => state.saveConnection);
  const probeConnection = useDbConnectionsStore((state) => state.probeConnection);

  const [name, setName] = useState(editing?.name ?? prefill?.name ?? '');
  const [engine, setEngine] = useState<DbEngineId>(editing?.engine ?? prefill?.engine ?? 'postgresql');
  const [host, setHost] = useState(editing?.host ?? prefill?.host ?? '');
  const [port, setPort] = useState(editing?.port ?? prefill?.port ?? 5432);
  const [username, setUsername] = useState(editing?.username ?? prefill?.username ?? '');
  const [password, setPassword] = useState('');
  const [database, setDatabase] = useState(editing?.defaultDatabase ?? prefill?.defaultDatabase ?? '');
  const [probing, setProbing] = useState(false);
  const [probeResult, setProbeResult] = useState<DbTestResult | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setPort(editing?.port ?? prefill?.port ?? ENGINE_OPTIONS.find((option) => option.id === engine)?.port ?? 5432);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine]);

  const buildInput = (): DbConnectionInput => ({
    id: editing?.id,
    name: name.trim() || host,
    engine,
    host: host.trim(),
    port,
    username: username.trim(),
    password,
    defaultDatabase: database.trim() || null,
  });

  const handleTest = async () => {
    if (!host.trim()) return;
    setProbing(true);
    setProbeResult(null);
    const result = await probeConnection(buildInput());
    setProbeResult(result);
    setProbing(false);
  };

  const handleSave = async () => {
    if (!host.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await saveConnection(buildInput());
      onSaved(saved);
      onClose();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSaving(false);
    }
  };

  const inputClass = 'h-9 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-2.5 text-sm text-tokyo-fg outline-none focus:border-tokyo-cyan';

  return (
    <div
      className="responsive-dialog-layer fixed inset-0 z-[120] flex items-center justify-center bg-tokyo-bg-dark/70 px-4"
      role="dialog"
      aria-modal="true"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <form
        className="w-full max-w-md rounded-xl border border-tokyo-bg-hl bg-tokyo-bg-dark p-5 shadow-2xl"
        onSubmit={(event) => {
          event.preventDefault();
          void handleSave();
        }}
      >
        <h3 className="text-sm font-semibold text-tokyo-fg">
          {editing ? t('plugins.dbConn.editTitle') : t('plugins.dbConn.addTitle')}
        </h3>

        <div className="mt-4 space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.name')}</span>
              <input className={inputClass} value={name} onChange={(event) => setName(event.target.value)} placeholder="Prod DB" />
            </label>
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.engine')}</span>
              <select
                className={inputClass}
                value={engine}
                onChange={(event) => setEngine(event.target.value as DbEngineId)}
              >
                {ENGINE_OPTIONS.map((option) => (
                  <option key={option.id} value={option.id}>{option.label}</option>
                ))}
              </select>
            </label>
          </div>
          <div className="grid grid-cols-[1fr_88px] gap-3">
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.host')}</span>
              <input className={cn(inputClass, 'font-mono')} value={host} onChange={(event) => setHost(event.target.value)} placeholder="db.example.com" />
            </label>
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.port')}</span>
              <input className={cn(inputClass, 'font-mono')} type="number" value={port} onChange={(event) => setPort(Number(event.target.value))} />
            </label>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.user')}</span>
              <input className={inputClass} value={username} onChange={(event) => setUsername(event.target.value)} placeholder="postgres" />
            </label>
            <label>
              <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.password')}</span>
              <input
                className={inputClass}
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={editing?.hasPassword ? t('plugins.dbConn.keepPassword') : ''}
              />
            </label>
          </div>
          <label>
            <span className="mb-1 block text-[10px] font-medium text-tokyo-comment">{t('plugins.dbConn.defaultDatabase')}</span>
            <input className={cn(inputClass, 'font-mono')} value={database} onChange={(event) => setDatabase(event.target.value)} placeholder="postgres (optional)" />
          </label>
        </div>

        {probeResult && (
          <div
            className={cn(
              'mt-3 rounded-md border px-3 py-2 text-xs',
              probeResult.ok
                ? 'border-tokyo-green/40 bg-tokyo-green/10 text-tokyo-green'
                : 'border-tokyo-red/40 bg-tokyo-red/10 text-tokyo-red'
            )}
          >
            {probeResult.ok
              ? `✓ ${probeResult.serverVersion ?? 'connected'} · ${probeResult.latencyMs} ms`
              : probeResult.error ?? 'failed'}
          </div>
        )}
        {error && (
          <div className="mt-3 rounded-md border border-tokyo-red/40 bg-tokyo-red/10 px-3 py-2 text-xs text-tokyo-red">
            {error}
          </div>
        )}

        <div className="mt-5 flex items-center justify-between">
          <button
            type="button"
            className="flex h-8 items-center gap-1.5 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg transition-colors hover:border-tokyo-cyan disabled:opacity-50"
            onClick={() => void handleTest()}
            disabled={probing || !host.trim()}
          >
            {probing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <PlugZap className="h-3.5 w-3.5" />}
            {t('plugins.dbConn.test')}
          </button>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="h-8 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 text-xs text-tokyo-fg hover:border-tokyo-comment"
              onClick={onClose}
            >
              {t('common.cancel')}
            </button>
            <button
              type="submit"
              className="h-8 rounded-md bg-tokyo-blue px-4 text-xs font-medium text-tokyo-on-accent hover:opacity-90 disabled:opacity-50"
              disabled={saving || !host.trim()}
            >
              {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : t('plugins.dbConn.save')}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
