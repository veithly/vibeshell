import { create } from 'zustand';
import { safeInvoke, TauriError } from '../lib/tauri';
import { useNotificationStore } from './notificationStore';

/**
 * Helper to show error notification
 */
function showError(title: string, error: TauriError): void {
  const { error: notifyError } = useNotificationStore.getState();
  if (!error.isTauriUnavailable) {
    notifyError(title, error.message);
  }
}

/**
 * Stored fingerprint from the backend
 */
export interface StoredFingerprint {
  id: string;
  host: string;
  port: number;
  fingerprint: string;
  algorithm: string;
  addedAt: number;
  lastVerifiedAt: number;
  serverName: string | null;
}

/**
 * Fingerprint verification result from backend
 */
export interface VerifyFingerprintResponse {
  status: 'trusted' | 'unknown' | 'changed';
  fingerprint: string | null;
  algorithm: string | null;
  storedFingerprint: string | null;
  storedAlgorithm: string | null;
  storedAt: number | null;
}

/**
 * Result of a handshake-only host-key probe (`probe_host_key`).
 * The probe performs key exchange + host-key check only — no authentication,
 * so no credentials ever reach the wire.
 */
export interface ProbeHostKeyResponse {
  status: 'known' | 'unknown' | 'changed';
  fingerprint: string | null;
  keyType: string | null;
  storedFingerprint: string | null;
  storedKeyType: string | null;
  storedAt: number | null;
}

/**
 * Pending verification request
 */
export interface PendingVerification {
  host: string;
  port: number;
  fingerprint: string;
  algorithm: string;
  serverName: string | null;
  status: 'unknown' | 'changed';
  storedFingerprint?: string;
  storedAlgorithm?: string;
  storedAt?: number;
  onAccept: () => void;
  onReject: () => void;
}

/**
 * Details parsed from a backend HOST_KEY_UNKNOWN / HOST_KEY_CHANGED error.
 * The structured error is emitted by the backend connect wrapper when the
 * handshake is refused by the host-key policy (e.g. a key that changed after
 * the probe, or an untrusted jump host).
 */
export interface HostKeyErrorInfo {
  kind: 'unknown' | 'changed';
  host?: string;
  port?: number;
  fingerprint?: string;
  keyType?: string;
  storedFingerprint?: string;
  storedKeyType?: string;
}

/**
 * Parse the machine-readable host-key rejection emitted by the backend.
 * Format: first line carries the HOST_KEY_* marker, following `key: value`
 * lines carry host/port/fingerprints (see HostKeyRejection::error_message).
 */
export function parseHostKeyError(message: string): HostKeyErrorInfo | null {
  const kind = message.match(/HOST_KEY_(UNKNOWN|CHANGED)/)?.[1];
  if (!kind) return null;
  const line = (name: string): string | undefined =>
    message.match(new RegExp(`^${name}: (.*)$`, 'm'))?.[1]?.trim();
  const port = line('port');
  return {
    kind: kind.toLowerCase() as 'unknown' | 'changed',
    host: line('host'),
    port: port ? Number(port) : undefined,
    fingerprint: line('presented-fingerprint'),
    keyType: line('presented-key-type'),
    storedFingerprint: line('stored-fingerprint'),
    storedKeyType: line('stored-key-type'),
  };
}

/**
 * Fingerprint store state and actions
 */
interface FingerprintStore {
  /** List of all stored fingerprints */
  fingerprints: StoredFingerprint[];
  /** Loading state */
  loading: boolean;
  /** Error message */
  error: string | null;
  /** Pending verification dialog */
  pendingVerification: PendingVerification | null;
  /** Whether the fingerprint manager dialog is open */
  managerOpen: boolean;

  /** Fetch all stored fingerprints */
  fetchFingerprints: () => Promise<void>;
  /** Get a specific fingerprint */
  getFingerprint: (host: string, port: number) => Promise<StoredFingerprint | null>;
  /** Verify a fingerprint */
  verifyFingerprint: (
    host: string,
    port: number,
    fingerprint: string,
    algorithm: string
  ) => Promise<VerifyFingerprintResponse>;
  /**
   * Probe a server's host key (handshake only, no credentials sent).
   * Returns null when the probe could not run (e.g. host unreachable);
   * the backend still enforces TOFU fail-closed on the real connect.
   */
  probeHostKey: (host: string, port: number) => Promise<ProbeHostKeyResponse | null>;
  /**
   * Open the host-key verification dialog and resolve once the user decides.
   * Resolves true when approved (the fingerprint is persisted by then via
   * acceptPendingVerification) and false when rejected.
   */
  requestHostKeyApproval: (
    verification: Omit<PendingVerification, 'onAccept' | 'onReject'>
  ) => Promise<boolean>;
  /** Save a fingerprint (trust it) */
  saveFingerprint: (
    host: string,
    port: number,
    fingerprint: string,
    algorithm: string,
    serverName?: string
  ) => Promise<boolean>;
  /** Delete a fingerprint */
  deleteFingerprint: (host: string, port: number) => Promise<boolean>;
  /** Delete a fingerprint by ID */
  deleteFingerprintById: (id: string) => Promise<boolean>;
  /** Clear all fingerprints */
  clearFingerprints: () => Promise<boolean>;
  /** Clear error */
  clearError: () => void;

  /** Set pending verification */
  setPendingVerification: (verification: PendingVerification | null) => void;
  /** Accept pending verification */
  acceptPendingVerification: () => Promise<void>;
  /** Reject pending verification */
  rejectPendingVerification: () => void;

  /** Open fingerprint manager */
  openManager: () => void;
  /** Close fingerprint manager */
  closeManager: () => void;
}

/**
 * Zustand store for managing SSH fingerprints
 */
export const useFingerprintStore = create<FingerprintStore>((set, get) => ({
  fingerprints: [],
  loading: false,
  error: null,
  pendingVerification: null,
  managerOpen: false,

  fetchFingerprints: async () => {
    set({ loading: true, error: null });

    const result = await safeInvoke<StoredFingerprint[]>('list_fingerprints');

    if (result.success) {
      set({ fingerprints: result.data, loading: false });
    } else {
      set({
        error: result.error.isTauriUnavailable
          ? 'Running in browser mode'
          : result.error.message,
        loading: false,
      });
      if (!result.error.isTauriUnavailable) {
        showError('Failed to Load Fingerprints', result.error);
      }
    }
  },

  getFingerprint: async (host: string, port: number) => {
    const result = await safeInvoke<StoredFingerprint | null>('get_fingerprint', {
      request: { host, port },
    });

    if (result.success) {
      return result.data;
    }
    return null;
  },

  probeHostKey: async (host: string, port: number) => {
    const result = await safeInvoke<ProbeHostKeyResponse>('probe_host_key', {
      request: { host, port },
    });

    if (result.success) {
      return result.data;
    }

    console.warn('[fingerprintStore] probe_host_key failed:', result.error.message);
    return null;
  },

  requestHostKeyApproval: (
    verification: Omit<PendingVerification, 'onAccept' | 'onReject'>
  ) => {
    return new Promise<boolean>((resolve) => {
      get().setPendingVerification({
        ...verification,
        onAccept: () => resolve(true),
        onReject: () => resolve(false),
      });
    });
  },

  verifyFingerprint: async (
    host: string,
    port: number,
    fingerprint: string,
    algorithm: string
  ) => {
    const result = await safeInvoke<VerifyFingerprintResponse>('verify_fingerprint', {
      request: { host, port, fingerprint, algorithm },
    });

    if (result.success) {
      return result.data;
    }

    // Return a default unknown response on error
    return {
      status: 'unknown' as const,
      fingerprint,
      algorithm,
      storedFingerprint: null,
      storedAlgorithm: null,
      storedAt: null,
    };
  },

  saveFingerprint: async (
    host: string,
    port: number,
    fingerprint: string,
    algorithm: string,
    serverName?: string
  ) => {
    const result = await safeInvoke<StoredFingerprint>('save_fingerprint', {
      request: { host, port, fingerprint, algorithm, serverName: serverName ?? null },
    });

    if (result.success) {
      // Refresh the fingerprints list
      await get().fetchFingerprints();
      return true;
    }

    showError('Failed to Save Fingerprint', result.error);
    return false;
  },

  deleteFingerprint: async (host: string, port: number) => {
    const result = await safeInvoke<boolean>('delete_fingerprint', {
      request: { host, port },
    });

    if (result.success) {
      // Refresh the fingerprints list
      await get().fetchFingerprints();
      return true;
    }

    showError('Failed to Delete Fingerprint', result.error);
    return false;
  },

  deleteFingerprintById: async (id: string) => {
    const result = await safeInvoke<boolean>('delete_fingerprint_by_id', {
      request: { id },
    });

    if (result.success) {
      // Refresh the fingerprints list
      await get().fetchFingerprints();
      return true;
    }

    showError('Failed to Delete Fingerprint', result.error);
    return false;
  },

  clearFingerprints: async () => {
    const result = await safeInvoke<void>('clear_fingerprints');

    if (result.success) {
      set({ fingerprints: [] });
      return true;
    }

    showError('Failed to Clear Fingerprints', result.error);
    return false;
  },

  clearError: () => {
    set({ error: null });
  },

  setPendingVerification: (verification) => {
    set({ pendingVerification: verification });
  },

  acceptPendingVerification: async () => {
    const pending = get().pendingVerification;
    if (!pending) return;

    // Save the fingerprint
    const saved = await get().saveFingerprint(
      pending.host,
      pending.port,
      pending.fingerprint,
      pending.algorithm,
      pending.serverName ?? undefined
    );

    if (saved) {
      // Call the accept callback
      pending.onAccept();
    } else {
      // Call reject on save failure
      pending.onReject();
    }

    set({ pendingVerification: null });
  },

  rejectPendingVerification: () => {
    const pending = get().pendingVerification;
    if (pending) {
      pending.onReject();
    }
    set({ pendingVerification: null });
  },

  openManager: () => {
    set({ managerOpen: true });
    // Fetch fresh data when opening
    get().fetchFingerprints();
  },

  closeManager: () => {
    set({ managerOpen: false });
  },
}));
