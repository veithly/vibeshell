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
