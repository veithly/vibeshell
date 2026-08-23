import { useEffect, useState } from 'react';
import { History, Loader2, Play, Search, Star, Trash2, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../../lib/utils';
import { useCommandHistoryStore } from '../../stores/commandHistoryStore';
import type { CommandHistoryEntry } from '../../types/commandHistory';

interface CommandHistoryDialogProps {
  isOpen: boolean;
  serverId?: string;
  serverName?: string;
  onClose: () => void;
  onUseCommand: (command: string) => void;
}

function formatUsedAt(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestamp * 1000));
}

export function CommandHistoryDialog({
  isOpen,
  serverId,
  serverName,
  onClose,
  onUseCommand,
}: CommandHistoryDialogProps) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState('');
  const [favoritesOnly, setFavoritesOnly] = useState(false);
  const entries = useCommandHistoryStore((state) => state.entries);
  const loading = useCommandHistoryStore((state) => state.loading);
  const error = useCommandHistoryStore((state) => state.error);
  const fetchHistory = useCommandHistoryStore((state) => state.fetchHistory);
  const setFavorite = useCommandHistoryStore((state) => state.setFavorite);
  const deleteEntry = useCommandHistoryStore((state) => state.deleteEntry);
  const clearHistory = useCommandHistoryStore((state) => state.clearHistory);
  const clearError = useCommandHistoryStore((state) => state.clearError);

  useEffect(() => {
    if (!isOpen) return;
    setQuery('');
    setFavoritesOnly(false);
    void fetchHistory(serverId ?? null);
  }, [fetchHistory, isOpen, serverId]);

  useEffect(() => {
    if (!isOpen) return;
    const timer = window.setTimeout(() => {
      void fetchHistory(serverId ?? null, query, favoritesOnly);
    }, 160);
    return () => window.clearTimeout(timer);
  }, [favoritesOnly, fetchHistory, isOpen, query, serverId]);

  const handleClear = async () => {
    if (!serverId) return;
    await clearHistory(serverId, false);
  };

  const handleClearAll = async () => {
    if (!serverId || entries.length === 0) return;
    if (!window.confirm(t('commandHistory.clearAllConfirm'))) return;
    await clearHistory(serverId, true);
  };

  const handleUse = (entry: CommandHistoryEntry) => {
    onUseCommand(entry.command);
    onClose();
  };

  const handleToggleFavorite = async (entry: CommandHistoryEntry) => {
    const updated = await setFavorite(entry.id, !entry.is_favorite);
    if (updated && serverId) {
      void fetchHistory(serverId, query, favoritesOnly);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <button
        type="button"
        aria-label={t('common.close')}
        className="absolute inset-0 cursor-default bg-tokyo-bg-dark/80"
        onClick={onClose}
      />

      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="command-history-title"
        className="relative flex max-h-[min(720px,calc(100vh-2rem))] w-full max-w-3xl flex-col overflow-hidden rounded-lg border border-tokyo-bg-hl bg-tokyo-bg shadow-2xl"
      >
        <header className="flex items-center justify-between border-b border-tokyo-bg-hl px-4 py-3">
          <div className="flex min-w-0 items-center gap-2">
            <History className="h-5 w-5 flex-shrink-0 text-tokyo-cyan" />
            <div className="min-w-0">
              <h2 id="command-history-title" className="truncate text-base font-semibold text-tokyo-fg">
                {t('commandHistory.title')}
              </h2>
              <p className="truncate text-xs text-tokyo-comment">
                {serverName || t('commandHistory.noServer')}
              </p>
            </div>
          </div>
          <button
            type="button"
            className="icon-button tooltip-button"
            data-tooltip={t('common.close')}
            aria-label={t('common.close')}
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </button>
        </header>

        <div className="flex flex-wrap items-center gap-2 border-b border-tokyo-bg-hl px-4 py-3">
          <label className="relative min-w-[180px] flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-tokyo-comment" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t('commandHistory.searchPlaceholder')}
              aria-label={t('common.search')}
              disabled={!serverId}
              className="w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark py-2 pl-8 pr-3 text-sm text-tokyo-fg outline-none placeholder:text-tokyo-comment focus:border-tokyo-blue focus:ring-1 focus:ring-tokyo-blue"
            />
          </label>
          <button
            type="button"
            className={cn(
              'flex items-center gap-1.5 rounded-md border px-2.5 py-2 text-sm transition-colors',
              favoritesOnly
                ? 'border-tokyo-yellow bg-tokyo-selection text-tokyo-yellow'
                : 'border-tokyo-bg-hl text-tokyo-comment hover:bg-tokyo-bg-hl hover:text-tokyo-fg'
            )}
            aria-pressed={favoritesOnly}
            onClick={() => setFavoritesOnly((current) => !current)}
            disabled={!serverId}
          >
            <Star className="h-4 w-4" fill={favoritesOnly ? 'currentColor' : 'none'} />
            <span>{t('commandHistory.favorites')}</span>
          </button>
          <button
            type="button"
            className="flex items-center gap-1.5 rounded-md border border-tokyo-bg-hl px-2.5 py-2 text-sm text-tokyo-comment transition-colors hover:bg-tokyo-bg-hl hover:text-tokyo-fg disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => void handleClear()}
            disabled={!serverId || loading || !entries.some((entry) => !entry.is_favorite)}
          >
            <Trash2 className="h-4 w-4" />
            <span>{t('commandHistory.clear')}</span>
          </button>
          <button
            type="button"
            className="flex items-center gap-1.5 rounded-md border border-tokyo-red/50 px-2.5 py-2 text-sm text-tokyo-red transition-colors hover:bg-tokyo-red/10 disabled:cursor-not-allowed disabled:opacity-50"
            onClick={() => void handleClearAll()}
            disabled={!serverId || loading || entries.length === 0}
          >
            <Trash2 className="h-4 w-4" />
            <span>{t('commandHistory.clearAll')}</span>
          </button>
        </div>

        {error && (
          <div className="flex items-center justify-between gap-3 border-b border-tokyo-red/40 bg-tokyo-red/10 px-4 py-2 text-sm text-tokyo-red">
            <span>{error}</span>
            <button type="button" className="text-xs underline" onClick={clearError}>
              {t('common.close')}
            </button>
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {!serverId ? (
            <div className="flex min-h-48 items-center justify-center px-6 text-center text-sm text-tokyo-comment">
              {t('commandHistory.connectFirst')}
            </div>
          ) : loading && entries.length === 0 ? (
            <div className="flex min-h-48 items-center justify-center text-tokyo-comment">
              <Loader2 className="h-5 w-5 animate-spin" aria-label={t('common.loading')} />
            </div>
          ) : entries.length === 0 ? (
            <div className="flex min-h-48 items-center justify-center px-6 text-center text-sm text-tokyo-comment">
              {query || favoritesOnly ? t('commandHistory.noMatches') : t('commandHistory.empty')}
            </div>
          ) : (
            <ul className="space-y-1">
              {entries.map((entry) => (
                <li
                  key={entry.id}
                  className="group flex items-start gap-2 rounded-md border border-transparent px-2.5 py-2 hover:border-tokyo-bg-hl hover:bg-tokyo-bg-dark"
                >
                  <button
                    type="button"
                    className="mt-0.5 flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-md text-tokyo-cyan opacity-70 transition-colors hover:bg-tokyo-selection hover:opacity-100"
                    aria-label={t('commandHistory.use')}
                    data-tooltip={t('commandHistory.use')}
                    onClick={() => handleUse(entry)}
                  >
                    <Play className="h-3.5 w-3.5" fill="currentColor" />
                  </button>
                  <button
                    type="button"
                    className="min-w-0 flex-1 text-left"
                    onClick={() => handleUse(entry)}
                    title={entry.command}
                  >
                    <code className="block whitespace-pre-wrap break-words font-mono text-sm text-tokyo-fg">
                      {entry.command}
                    </code>
                    <span className="mt-1 block text-[11px] text-tokyo-comment">
                      {t('commandHistory.used', { count: entry.use_count })} · {formatUsedAt(entry.last_used_at, i18n.language)}
                    </span>
                  </button>
                  <button
                    type="button"
                    className={cn(
                      'icon-button h-7 w-7 flex-shrink-0',
                      entry.is_favorite ? 'text-tokyo-yellow' : 'text-tokyo-comment opacity-60 group-hover:opacity-100'
                    )}
                    aria-label={entry.is_favorite ? t('commandHistory.unfavorite') : t('commandHistory.favorite')}
                    data-tooltip={entry.is_favorite ? t('commandHistory.unfavorite') : t('commandHistory.favorite')}
                    aria-pressed={entry.is_favorite}
                    onClick={() => void handleToggleFavorite(entry)}
                  >
                    <Star className="h-4 w-4" fill={entry.is_favorite ? 'currentColor' : 'none'} />
                  </button>
                  <button
                    type="button"
                    className="icon-button h-7 w-7 flex-shrink-0 text-tokyo-comment opacity-60 group-hover:opacity-100 hover:text-tokyo-red"
                    aria-label={t('common.delete')}
                    data-tooltip={t('common.delete')}
                    onClick={() => void deleteEntry(entry.id)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>
    </div>
  );
}
