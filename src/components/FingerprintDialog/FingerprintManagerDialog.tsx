import { useEffect, useCallback, useState } from 'react';
import { X, Trash2, Shield, Key, Clock, Server, RefreshCw } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useFingerprintStore, StoredFingerprint } from '../../stores/fingerprintStore';
import { ConfirmDialog } from '../ConfirmDialog/ConfirmDialog';

/**
 * Dialog for managing stored SSH host key fingerprints
 * Allows viewing and deleting trusted server fingerprints
 */
export function FingerprintManagerDialog() {
  const {
    fingerprints,
    loading,
    managerOpen,
    closeManager,
    fetchFingerprints,
    deleteFingerprintById,
    clearFingerprints,
  } = useFingerprintStore();

  const [deleteConfirm, setDeleteConfirm] = useState<StoredFingerprint | null>(null);
  const [clearConfirm, setClearConfirm] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  // Fetch fingerprints when dialog opens
  useEffect(() => {
    if (managerOpen) {
      fetchFingerprints();
    }
  }, [managerOpen, fetchFingerprints]);

  const handleDelete = useCallback(async () => {
    if (!deleteConfirm) return;

    setIsDeleting(true);
    try {
      await deleteFingerprintById(deleteConfirm.id);
    } finally {
      setIsDeleting(false);
      setDeleteConfirm(null);
    }
  }, [deleteConfirm, deleteFingerprintById]);

  const handleClearAll = useCallback(async () => {
    setIsDeleting(true);
    try {
      await clearFingerprints();
    } finally {
      setIsDeleting(false);
      setClearConfirm(false);
    }
  }, [clearFingerprints]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      closeManager();
    }
  }, [closeManager]);

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  if (!managerOpen) return null;

  return (
    <>
      <div
        className="fixed inset-0 z-50 flex items-center justify-center"
        onKeyDown={handleKeyDown}
      >
        {/* Backdrop */}
        <div
          className="absolute inset-0 bg-black/60"
          onClick={closeManager}
        />

        {/* Dialog */}
        <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-2xl mx-4 max-h-[80vh] flex flex-col">
          {/* Header */}
          <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl flex-shrink-0">
            <div className="flex items-center gap-3">
              <Shield className="w-5 h-5 text-tokyo-blue" />
              <h2 className="text-lg font-semibold text-white">SSH Host Key Manager</h2>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => fetchFingerprints()}
                disabled={loading}
                className="p-1.5 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors disabled:opacity-50"
                title="Refresh"
              >
                <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
              </button>
              <button
                className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
                onClick={closeManager}
              >
                <X className="w-5 h-5" />
              </button>
            </div>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-auto p-4">
            {loading ? (
              <div className="flex items-center justify-center py-12">
                <RefreshCw className="w-6 h-6 text-tokyo-comment animate-spin" />
              </div>
            ) : fingerprints.length === 0 ? (
              <div className="text-center py-12">
                <Shield className="w-12 h-12 text-tokyo-comment mx-auto mb-4" />
                <p className="text-tokyo-fg">No trusted host keys stored</p>
                <p className="text-sm text-tokyo-comment mt-2">
                  Host keys will be added when you connect to servers for the first time.
                </p>
              </div>
            ) : (
              <div className="space-y-3">
                {fingerprints.map((fp) => (
                  <div
                    key={fp.id}
                    className="p-4 rounded-lg bg-tokyo-bg border border-tokyo-bg-hl hover:border-tokyo-blue/30 transition-colors"
                  >
                    <div className="flex items-start justify-between gap-4">
                      <div className="flex-1 min-w-0">
                        {/* Server info */}
                        <div className="flex items-center gap-2 mb-2">
                          <Server className="w-4 h-4 text-tokyo-blue flex-shrink-0" />
                          <span className="font-medium text-white truncate">
                            {fp.serverName || `${fp.host}:${fp.port}`}
                          </span>
                          {fp.serverName && (
                            <span className="text-sm text-tokyo-comment">
                              ({fp.host}:{fp.port})
                            </span>
                          )}
                        </div>

                        {/* Fingerprint */}
                        <div className="flex items-start gap-2 mb-2">
                          <Key className="w-4 h-4 text-tokyo-green flex-shrink-0 mt-0.5" />
                          <div className="min-w-0">
                            <span className="text-xs text-tokyo-comment">{fp.algorithm}</span>
                            <code className="block text-xs text-tokyo-fg font-mono break-all mt-0.5">
                              {fp.fingerprint}
                            </code>
                          </div>
                        </div>

                        {/* Timestamps */}
                        <div className="flex items-center gap-4 text-xs text-tokyo-comment">
                          <div className="flex items-center gap-1">
                            <Clock className="w-3 h-3" />
                            <span>Added: {formatDate(fp.addedAt)}</span>
                          </div>
                          {fp.lastVerifiedAt !== fp.addedAt && (
                            <div className="flex items-center gap-1">
                              <Shield className="w-3 h-3" />
                              <span>Last verified: {formatDate(fp.lastVerifiedAt)}</span>
                            </div>
                          )}
                        </div>
                      </div>

                      {/* Delete button */}
                      <button
                        onClick={() => setDeleteConfirm(fp)}
                        className="p-2 rounded-md text-tokyo-comment hover:text-tokyo-red hover:bg-tokyo-red/10 transition-colors flex-shrink-0"
                        title="Delete fingerprint"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Footer */}
          {fingerprints.length > 0 && (
            <div className="flex justify-between items-center px-4 py-3 border-t border-tokyo-bg-hl flex-shrink-0">
              <p className="text-sm text-tokyo-comment">
                {fingerprints.length} trusted host{fingerprints.length !== 1 ? 's' : ''}
              </p>
              <button
                onClick={() => setClearConfirm(true)}
                className={cn(
                  'flex items-center gap-2 px-3 py-1.5 rounded-md text-sm',
                  'text-tokyo-red hover:bg-tokyo-red/10',
                  'transition-colors'
                )}
              >
                <Trash2 className="w-4 h-4" />
                Clear All
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Delete confirmation dialog */}
      <ConfirmDialog
        isOpen={!!deleteConfirm}
        title="Delete Fingerprint"
        message={deleteConfirm
          ? `Are you sure you want to remove the trusted fingerprint for ${deleteConfirm.serverName || `${deleteConfirm.host}:${deleteConfirm.port}`}? You will need to verify the host key again on the next connection.`
          : ''
        }
        confirmLabel={isDeleting ? 'Deleting...' : 'Delete'}
        variant="danger"
        onConfirm={handleDelete}
        onCancel={() => setDeleteConfirm(null)}
      />

      {/* Clear all confirmation dialog */}
      <ConfirmDialog
        isOpen={clearConfirm}
        title="Clear All Fingerprints"
        message="Are you sure you want to remove all trusted host key fingerprints? You will need to verify each server again on the next connection."
        confirmLabel={isDeleting ? 'Clearing...' : 'Clear All'}
        variant="danger"
        onConfirm={handleClearAll}
        onCancel={() => setClearConfirm(false)}
      />
    </>
  );
}
