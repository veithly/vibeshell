import { type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Loader2, RefreshCw, X } from 'lucide-react';
import { cn } from '../../lib/utils';

export function ErrorBanner({ message, onDismiss }: { message: string | null; onDismiss?: () => void }) {
  if (!message) return null;
  return (
    <div className="m-2 flex items-center gap-2 rounded-md border border-tokyo-red/30 bg-tokyo-red/10 px-3 py-2 text-xs text-tokyo-red">
      <AlertTriangle className="h-3.5 w-3.5 flex-shrink-0" />
      <span className="min-w-0 flex-1 break-all">{message}</span>
      {onDismiss && (
        <button className="underline" onClick={onDismiss}>
          <X className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}

export function StatCard({
  label,
  value,
  tone = 'default',
}: {
  label: string;
  value: ReactNode;
  tone?: 'default' | 'good' | 'warn' | 'bad';
}) {
  return (
    <div className="min-w-[110px] flex-1 rounded-lg border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2">
      <div className="truncate text-[10px] uppercase tracking-wide text-tokyo-comment">{label}</div>
      <div
        className={cn(
          'mt-0.5 truncate text-sm font-semibold tabular-nums',
          tone === 'good' && 'text-tokyo-green',
          tone === 'warn' && 'text-tokyo-yellow',
          tone === 'bad' && 'text-tokyo-red',
          tone === 'default' && 'text-tokyo-fg'
        )}
      >
        {value}
      </div>
    </div>
  );
}

export interface DashboardTab {
  id: string;
  label: string;
}

export function DashboardHeader({
  icon,
  title,
  badge,
  tabs,
  activeTab,
  onTabChange,
  onRefresh,
  refreshing,
  extra,
}: {
  icon: ReactNode;
  title: string;
  badge?: string | null;
  tabs?: DashboardTab[];
  activeTab?: string;
  onTabChange?: (tab: string) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  extra?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-b border-tokyo-bg-hl px-3 py-2">
      <div className="flex items-center gap-2">
        {icon}
        <span className="text-sm font-semibold text-tokyo-fg">{title}</span>
        {badge && (
          <span className="rounded border border-tokyo-bg-hl bg-tokyo-bg-dark px-1.5 py-0.5 text-[10px] text-tokyo-comment">
            {badge}
          </span>
        )}
      </div>
      {tabs && tabs.length > 0 && (
        <div className="flex h-7 rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark p-0.5">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={cn(
                'rounded px-2.5 text-xs transition-colors',
                activeTab === tab.id
                  ? 'bg-tokyo-bg-hl text-tokyo-fg'
                  : 'text-tokyo-comment hover:text-tokyo-fg'
              )}
              onClick={() => onTabChange?.(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>
      )}
      <div className="ml-auto flex items-center gap-1">
        {extra}
        {onRefresh && (
          <button
            className="icon-button h-7 w-7"
            onClick={onRefresh}
            aria-label="refresh"
            title="refresh"
          >
            <RefreshCw className={cn('h-3.5 w-3.5', refreshing && 'animate-spin')} />
          </button>
        )}
      </div>
    </div>
  );
}

export function CenterNotice({ text, loading }: { text: string; loading?: boolean }) {
  return (
    <div className="flex h-full min-h-[120px] items-center justify-center gap-2 text-xs text-tokyo-comment">
      {loading && <Loader2 className="h-4 w-4 animate-spin" />}
      {text}
    </div>
  );
}

export function DashboardModal({
  title,
  onClose,
  children,
  wide,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div
      className="responsive-dialog-layer fixed inset-0 z-[120] flex items-center justify-center bg-tokyo-bg-dark/70 px-4 py-8"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className={cn(
          'flex max-h-full min-h-0 w-full flex-col rounded-lg border border-tokyo-bg-hl bg-tokyo-bg-dark shadow-2xl',
          wide ? 'max-w-4xl' : 'max-w-2xl'
        )}
      >
        <div className="flex h-10 flex-shrink-0 items-center border-b border-tokyo-bg-hl px-3">
          <span className="min-w-0 flex-1 truncate text-sm font-semibold text-tokyo-fg">{title}</span>
          <button className="icon-button h-7 w-7" onClick={onClose} aria-label={t('common.close')}>
            <X className="h-4 w-4" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-3">{children}</div>
      </div>
    </div>
  );
}

export function DataGrid({
  columns,
  rows,
  emptyText,
  maxCellWidth = 260,
}: {
  columns: string[];
  rows: string[][];
  emptyText: string;
  maxCellWidth?: number;
}) {
  if (rows.length === 0) {
    return <CenterNotice text={emptyText} />;
  }
  return (
    <div className="m-2 overflow-hidden rounded-lg border border-tokyo-bg-hl">
      <table className="w-full border-separate border-spacing-0 text-left text-xs">
        <thead className="bg-tokyo-bg-dark text-tokyo-comment">
          <tr>
            {columns.map((column) => (
              <th key={column} className="border-b border-tokyo-bg-hl px-3 py-2 font-medium">
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="font-mono text-tokyo-fg">
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex} className="hover:bg-tokyo-bg-hl/40">
              {columns.map((_, cellIndex) => (
                <td
                  key={cellIndex}
                  className="border-b border-tokyo-bg-hl/60 px-3 py-1.5 align-top"
                  style={{ maxWidth: maxCellWidth }}
                >
                  <span className="block truncate" title={row[cellIndex] ?? ''}>
                    {row[cellIndex] || '-'}
                  </span>
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
