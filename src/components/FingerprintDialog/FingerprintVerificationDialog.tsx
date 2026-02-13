import { useCallback } from 'react';
import { X, AlertTriangle, ShieldAlert, ShieldCheck, Key, Copy, Check } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useFingerprintStore, PendingVerification } from '../../stores/fingerprintStore';
import { useState } from 'react';

/**
 * Dialog for verifying SSH host key fingerprints
 * Shows when connecting to a new server or when fingerprint has changed
 */
export function FingerprintVerificationDialog() {
  const {
    pendingVerification,
    acceptPendingVerification,
    rejectPendingVerification,
  } = useFingerprintStore();

  const [copied, setCopied] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);

  const handleAccept = useCallback(async () => {
    setIsProcessing(true);
    try {
      await acceptPendingVerification();
    } finally {
      setIsProcessing(false);
    }
  }, [acceptPendingVerification]);

  const handleReject = useCallback(() => {
    rejectPendingVerification();
  }, [rejectPendingVerification]);

  const handleCopyFingerprint = useCallback(() => {
    if (pendingVerification) {
      navigator.clipboard.writeText(pendingVerification.fingerprint);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [pendingVerification]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      handleReject();
    }
  }, [handleReject]);

  if (!pendingVerification) return null;

  const isChanged = pendingVerification.status === 'changed';

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      onKeyDown={handleKeyDown}
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/70"
        onClick={handleReject}
      />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-lg mx-4">
        {/* Header */}
        <div className={cn(
          "flex items-center justify-between px-4 py-3 border-b",
          isChanged ? "border-tokyo-red/50 bg-tokyo-red/10" : "border-tokyo-yellow/50 bg-tokyo-yellow/10"
        )}>
          <div className="flex items-center gap-3">
            {isChanged ? (
              <ShieldAlert className="w-6 h-6 text-tokyo-red" />
            ) : (
              <ShieldCheck className="w-6 h-6 text-tokyo-yellow" />
            )}
            <h2 className={cn(
              "text-lg font-semibold",
              isChanged ? "text-tokyo-red" : "text-tokyo-yellow"
            )}>
              {isChanged ? 'Host Key Changed!' : 'New Host Key'}
            </h2>
          </div>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
            onClick={handleReject}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4">
          {/* Warning for changed fingerprint */}
          {isChanged && (
            <div className="p-3 rounded-md bg-tokyo-red/10 border border-tokyo-red/30 flex items-start gap-3">
              <AlertTriangle className="w-5 h-5 text-tokyo-red flex-shrink-0 mt-0.5" />
              <div className="text-sm text-tokyo-red">
                <p className="font-semibold">Warning: Potential Security Risk!</p>
                <p className="mt-1 text-tokyo-red/80">
                  The host key for this server has changed since your last connection.
                  This could indicate a man-in-the-middle attack, or the server was reinstalled.
                </p>
              </div>
            </div>
          )}

          {/* Server info */}
          <div className="p-3 rounded-md bg-tokyo-bg border border-tokyo-bg-hl">
            <div className="text-sm text-tokyo-comment">Connecting to</div>
            <div className="text-white font-medium">
              {pendingVerification.serverName || `${pendingVerification.host}:${pendingVerification.port}`}
            </div>
            {pendingVerification.serverName && (
              <div className="text-sm text-tokyo-comment mt-1">
                {pendingVerification.host}:{pendingVerification.port}
              </div>
            )}
          </div>

          {/* New fingerprint */}
          <div>
            <label className="block text-sm font-medium text-tokyo-fg mb-2">
              <Key className="w-4 h-4 inline-block mr-1" />
              {isChanged ? 'New ' : ''}Server Fingerprint ({pendingVerification.algorithm})
            </label>
            <div className="flex items-center gap-2">
              <code className={cn(
                "flex-1 p-2 rounded-md font-mono text-xs break-all",
                "bg-tokyo-bg border border-tokyo-bg-hl",
                isChanged ? "text-tokyo-red" : "text-tokyo-green"
              )}>
                {pendingVerification.fingerprint}
              </code>
              <button
                onClick={handleCopyFingerprint}
                className="p-2 rounded-md bg-tokyo-bg-hl text-tokyo-fg hover:text-white transition-colors"
                title="Copy fingerprint"
              >
                {copied ? <Check className="w-4 h-4 text-tokyo-green" /> : <Copy className="w-4 h-4" />}
              </button>
            </div>
          </div>

          {/* Previous fingerprint (for changed) */}
          {isChanged && pendingVerification.storedFingerprint && (
            <div>
              <label className="block text-sm font-medium text-tokyo-comment mb-2">
                Previously Stored Fingerprint ({pendingVerification.storedAlgorithm})
              </label>
              <code className="block p-2 rounded-md font-mono text-xs break-all bg-tokyo-bg border border-tokyo-bg-hl text-tokyo-comment">
                {pendingVerification.storedFingerprint}
              </code>
              {pendingVerification.storedAt && (
                <p className="text-xs text-tokyo-comment mt-1">
                  Stored on {new Date(pendingVerification.storedAt * 1000).toLocaleDateString()}
                </p>
              )}
            </div>
          )}

          {/* Instructions */}
          <p className="text-sm text-tokyo-fg">
            {isChanged ? (
              <>
                If you expected this change (e.g., server reinstallation), you can accept the new fingerprint.
                Otherwise, <span className="text-tokyo-red font-semibold">do not connect</span> and verify with your server administrator.
              </>
            ) : (
              <>
                Please verify this fingerprint matches your server&apos;s host key before connecting.
                You can typically find this by running <code className="text-tokyo-blue">ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub</code> on the server.
              </>
            )}
          </p>
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-3 px-4 py-3 border-t border-tokyo-bg-hl">
          <button
            onClick={handleReject}
            disabled={isProcessing}
            className={cn(
              'px-4 py-2 rounded-md',
              'bg-tokyo-bg-hl text-tokyo-fg',
              'hover:bg-tokyo-bg hover:text-white',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              'transition-colors'
            )}
          >
            Cancel
          </button>
          <button
            onClick={handleAccept}
            disabled={isProcessing}
            className={cn(
              'px-4 py-2 rounded-md',
              'transition-colors',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              isChanged
                ? 'bg-tokyo-red text-white hover:bg-tokyo-red/80'
                : 'bg-tokyo-green text-white hover:bg-tokyo-green/80'
            )}
          >
            {isProcessing ? 'Saving...' : isChanged ? 'Accept New Key' : 'Trust & Connect'}
          </button>
        </div>
      </div>
    </div>
  );
}

export type { PendingVerification };
