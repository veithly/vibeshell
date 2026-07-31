import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, Clock, ShieldX, ShieldCheck } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useAgentApprovalStore } from '../../stores/agentApprovalStore';

/**
 * Modal that gates dangerous agent commands. Shows the head of the approval
 * queue and offers deny / allow-once / allow-and-auto-confirm actions.
 *
 * The backdrop is intentionally inert (no click-to-dismiss) so a decision is
 * always explicit; Escape denies as the safe default.
 */
export function AgentApprovalDialog() {
  const { t } = useTranslation();
  const request = useAgentApprovalStore((s) => s.queue[0] ?? null);
  const queueLength = useAgentApprovalStore((s) => s.queue.length);
  const autoApproveUntil = useAgentApprovalStore((s) => s.autoApproveUntil);
  const resolvingIds = useAgentApprovalStore((s) => s.resolvingIds);
  const error = useAgentApprovalStore((s) => s.error);
  const initialize = useAgentApprovalStore((s) => s.initialize);
  const approveOnce = useAgentApprovalStore((s) => s.approveOnce);
  const deny = useAgentApprovalStore((s) => s.deny);
  const approveWithAutoConfirm = useAgentApprovalStore((s) => s.approveWithAutoConfirm);
  const cancelAutoApprove = useAgentApprovalStore((s) => s.cancelAutoApprove);

  const denyButtonRef = useRef<HTMLButtonElement>(null);
  const expiredWindowHandledRef = useRef<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const isResolving = request ? resolvingIds.includes(request.id) : false;
  const autoApproveActive = autoApproveUntil !== null && autoApproveUntil > now;

  useEffect(() => {
    void initialize();
  }, [initialize]);

  useEffect(() => {
    if (!autoApproveUntil) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(interval);
  }, [autoApproveUntil]);

  useEffect(() => {
    if (autoApproveUntil !== null && autoApproveUntil <= now) {
      if (expiredWindowHandledRef.current !== autoApproveUntil) {
        expiredWindowHandledRef.current = autoApproveUntil;
        void cancelAutoApprove();
      }
    } else {
      expiredWindowHandledRef.current = null;
    }
  }, [autoApproveUntil, cancelAutoApprove, now]);

  useEffect(() => {
    if (request) {
      const timer = setTimeout(() => denyButtonRef.current?.focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [request]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (request && e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        void deny(request.id);
      }
    },
    [request, deny]
  );

  return (
    <>
      {autoApproveActive && autoApproveUntil && (
        <div
          role="status"
          className="fixed right-4 top-12 z-[55] flex max-w-sm items-center gap-3 rounded-lg border border-tokyo-yellow/50 bg-tokyo-bg-dark px-3 py-2 shadow-xl"
        >
          <Clock className="h-4 w-4 flex-shrink-0 text-tokyo-yellow" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-tokyo-yellow">{t('agentApproval.autoActive')}</p>
            <p className="text-xs text-tokyo-comment">
              {t('agentApproval.autoActiveUntil', {
                time: new Date(autoApproveUntil).toLocaleTimeString(),
              })}
            </p>
          </div>
          <button
            type="button"
            onClick={() => void cancelAutoApprove()}
            className="rounded-md px-2 py-1 text-xs text-tokyo-fg transition-colors hover:bg-tokyo-bg-hl"
          >
            {t('agentApproval.cancelAuto')}
          </button>
        </div>
      )}

      {request && (
        <div
          className="fixed inset-0 z-[60] flex items-center justify-center"
          onKeyDown={handleKeyDown}
        >
          <div className="absolute inset-0 bg-tokyo-bg-dark/80" />

          <div
            role="alertdialog"
            aria-modal="true"
            aria-describedby="agent-approval-description"
            className="relative mx-4 w-full max-w-lg rounded-lg border border-tokyo-red/50 bg-tokyo-bg-dark shadow-xl"
          >
        <div className="flex items-center gap-2 border-b border-tokyo-bg-hl px-4 py-3">
          <AlertTriangle className="h-5 w-5 text-tokyo-red" aria-hidden="true" />
          <h2 className="text-lg font-semibold text-tokyo-fg">{t('agentApproval.title')}</h2>
        </div>

        <div className="space-y-3 p-4">
          <p id="agent-approval-description" className="text-sm text-tokyo-comment">
            {t('agentApproval.description')}
          </p>

          {queueLength > 1 && (
            <p className="text-xs text-tokyo-yellow">
              {t('agentApproval.pendingCount', { count: queueLength })}
            </p>
          )}

          {error && (
            <p role="alert" className="rounded-md border border-tokyo-red/40 bg-tokyo-red/10 px-3 py-2 text-sm text-tokyo-red">
              {error}
            </p>
          )}

          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-tokyo-comment">
            <span>
              <span className="text-tokyo-comment">{t('agentApproval.toolLabel')}: </span>
              <span className="font-mono text-tokyo-fg">{request.tool}</span>
            </span>
            {request.sessionId && (
              <span>
                <span className="text-tokyo-comment">{t('agentApproval.sessionLabel')}: </span>
                <span className="font-mono text-tokyo-fg">{request.sessionId}</span>
              </span>
            )}
          </div>

          <div>
            <p className="mb-1 text-xs font-medium text-tokyo-comment">
              {t('agentApproval.commandLabel')}
            </p>
            <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 font-mono text-sm text-tokyo-fg">
              {request.command}
            </pre>
          </div>

          {request.reasons.length > 0 && (
            <div>
              <p className="mb-1 text-xs font-medium text-tokyo-comment">
                {t('agentApproval.reasonsLabel')}
              </p>
              <ul className="list-inside list-disc space-y-0.5 text-sm text-tokyo-red">
                {request.reasons.map((reason, index) => (
                  <li key={index}>{reason}</li>
                ))}
              </ul>
            </div>
          )}
        </div>

        <div className="flex flex-wrap justify-end gap-2 border-t border-tokyo-bg-hl px-4 py-3">
          <button
            ref={denyButtonRef}
            type="button"
            disabled={isResolving}
            onClick={() => void deny(request.id)}
            className="inline-flex items-center gap-1.5 rounded-md bg-tokyo-bg-hl px-3 py-2 text-sm text-tokyo-fg transition-colors hover:bg-tokyo-bg"
          >
            <ShieldX className="h-4 w-4" />
            {t('agentApproval.deny')}
          </button>
          <button
            type="button"
            disabled={isResolving}
            onClick={() => void approveOnce(request.id)}
            className="inline-flex items-center gap-1.5 rounded-md bg-tokyo-blue px-3 py-2 text-sm text-tokyo-on-accent transition-colors hover:bg-tokyo-blue/80"
          >
            <ShieldCheck className="h-4 w-4" />
            {t('agentApproval.approveOnce')}
          </button>
          <button
            type="button"
            disabled={isResolving}
            onClick={() => void approveWithAutoConfirm(request.id)}
            className={cn(
              'inline-flex items-center gap-1.5 rounded-md border border-tokyo-yellow/60 px-3 py-2 text-sm text-tokyo-yellow',
              'transition-colors hover:bg-tokyo-yellow/10'
            )}
          >
            <Clock className="h-4 w-4" />
            {t('agentApproval.approveAuto')}
          </button>
          </div>
        </div>
        </div>
      )}
    </>
  );
}
