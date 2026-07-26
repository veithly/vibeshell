/**
 * Tauri API utilities for VibeShell
 * Provides safe wrappers around Tauri commands with proper error handling
 *
 * PERFORMANCE OPTIMIZATIONS:
 * - Caches invoke function after first import to avoid repeated dynamic imports
 * - Provides fire-and-forget functions for performance-critical paths (input handling)
 * - Supports input batching for rapid keystroke handling
 */

/**
 * Custom error class for Tauri-related errors
 */
export class TauriError extends Error {
  public readonly command: string;
  public readonly originalError: unknown;
  public readonly isTauriUnavailable: boolean;

  constructor(command: string, originalError: unknown, isTauriUnavailable = false) {
    const message = isTauriUnavailable
      ? `Tauri API is not available. Running in browser mode.`
      : `Tauri command "${command}" failed: ${extractErrorMessage(originalError)}`;

    super(message);
    this.name = 'TauriError';
    this.command = command;
    this.originalError = originalError;
    this.isTauriUnavailable = isTauriUnavailable;
  }
}

/**
 * Extract a readable error message from an unknown error
 */
function extractErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

/**
 * Cached Tauri availability status
 */
let tauriAvailabilityCache: boolean | null = null;
let tauriCheckPromise: Promise<boolean> | null = null;

/**
 * Cached invoke function for performance - avoids repeated dynamic imports
 */
let cachedInvoke: (<T>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null = null;

/**
 * Get the cached invoke function, importing it if necessary
 * This is critical for input performance - we don't want to dynamically import on every keystroke
 */
async function getCachedInvoke(): Promise<typeof cachedInvoke> {
  if (cachedInvoke !== null) {
    return cachedInvoke;
  }

  const available = await isTauriAvailable();
  if (!available) {
    return null;
  }

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    cachedInvoke = invoke;
    return invoke;
  } catch {
    return null;
  }
}

/**
 * Check if Tauri API is available
 * This caches the result after the first check for performance
 */
export async function isTauriAvailable(): Promise<boolean> {
  // Return cached result if available
  if (tauriAvailabilityCache !== null) {
    return tauriAvailabilityCache;
  }

  // If a check is already in progress, wait for it
  if (tauriCheckPromise !== null) {
    return tauriCheckPromise;
  }

  // Perform the check
  tauriCheckPromise = (async () => {
    try {
      // Check if window.__TAURI_INTERNALS__ exists (Tauri v2 indicator)
      if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
        // Try to import and verify the API works
        const { invoke } = await import('@tauri-apps/api/core');
        // Try a simple ping command or just verify invoke exists
        if (typeof invoke === 'function') {
          // Cache the invoke function while we're here
          cachedInvoke = invoke;
          tauriAvailabilityCache = true;
          return true;
        }
      }
      tauriAvailabilityCache = false;
      return false;
    } catch {
      tauriAvailabilityCache = false;
      return false;
    }
  })();

  return tauriCheckPromise;
}

/**
 * Synchronously check if Tauri is available (after initial async check)
 * Returns null if the check hasn't been performed yet
 */
export function isTauriAvailableSync(): boolean | null {
  return tauriAvailabilityCache;
}

/**
 * Reset the Tauri availability cache (useful for testing)
 */
export function resetTauriAvailabilityCache(): void {
  tauriAvailabilityCache = null;
  tauriCheckPromise = null;
}

/**
 * Result type for safeInvoke operations
 */
export type InvokeResult<T> =
  | { success: true; data: T }
  | { success: false; error: TauriError };

/**
 * Safe invoke wrapper that properly handles and reports errors
 * Instead of silently returning null, it returns a Result type
 * that clearly indicates success or failure with detailed error information
 *
 * OPTIMIZATION: Uses cached invoke function to avoid repeated dynamic imports
 */
export async function safeInvoke<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<InvokeResult<T>> {
  try {
    // Use cached invoke function for better performance
    const invoke = await getCachedInvoke();
    if (!invoke) {
      return {
        success: false,
        error: new TauriError(command, 'Tauri API not available', true),
      };
    }

    const result = await invoke<T>(command, args);
    return { success: true, data: result };
  } catch (error) {
    console.error(`Tauri command "${command}" failed:`, error);
    return {
      success: false,
      error: new TauriError(command, error),
    };
  }
}

/**
 * Fire-and-forget invoke for performance-critical paths like input handling.
 * Does NOT await the result - caller is not notified of success/failure.
 * Use this only when you don't need the result and latency is critical.
 *
 * PERFORMANCE: Minimal overhead - uses cached invoke, no awaiting, no error handling overhead
 */
export function fireAndForgetInvoke(
  command: string,
  args?: Record<string, unknown>
): void {
  // If invoke is already cached, use it directly (synchronous path for best perf)
  if (cachedInvoke) {
    cachedInvoke(command, args).catch(() => {
      // Silently ignore errors for fire-and-forget calls
      // Could optionally log in development mode
    });
    return;
  }

  // Fall back to async path if not yet cached
  getCachedInvoke().then((invoke) => {
    if (invoke) {
      invoke(command, args).catch(() => {
        // Silently ignore
      });
    }
  });
}

/**
 * Input batching state for combining rapid keystrokes
 */
interface InputBatchState {
  command: string;
  sessionId: string;
  data: string;
  rafId: number | null;
}

// Per-session batch map. Each terminal pane owns its own batch so that input
// from multiple simultaneous panes never cross-contaminates (the previous
// module-level singleton flushed pane A's buffered keystrokes the moment pane
// B typed, and left dangling RAFs after unmount).
const inputBatches = new Map<string, InputBatchState>();

/**
 * Batched input send using requestAnimationFrame.
 * Combines rapid keystrokes into a single IPC call for better performance.
 *
 * PERFORMANCE: Groups multiple keystrokes within a single animation frame.
 * Batches are tracked per session so concurrent panes do not interfere.
 */
export function sendInputBatched(
  sessionId: string,
  data: string,
  command = 'session_send_input'
): void {
  const batch = inputBatches.get(sessionId);

  if (batch) {
    // Command should not change for an existing session batch, but guard anyway.
    if (batch.command !== command) {
      flushInputBatch(sessionId);
      inputBatches.set(sessionId, { command, sessionId, data, rafId: null });
    } else {
      batch.data += data;
    }
  } else {
    inputBatches.set(sessionId, { command, sessionId, data, rafId: null });
  }

  const current = inputBatches.get(sessionId)!;
  // Schedule flush on next animation frame if not already scheduled.
  if (current.rafId === null) {
    current.rafId = requestAnimationFrame(() => flushInputBatch(sessionId));
  }
}

/**
 * Flush the input batch immediately.
 *
 * Pass a sessionId to flush only that session's pending batch (used on
 * terminal unmount). Omit it to flush every active batch.
 */
export function flushInputBatch(sessionId?: string): void {
  const flushOne = (key: string) => {
    const batch = inputBatches.get(key);
    if (!batch) return;

    if (batch.rafId !== null) {
      cancelAnimationFrame(batch.rafId);
      batch.rafId = null;
    }

    if (batch.data && batch.sessionId) {
      const command = batch.command;
      const sid = batch.sessionId;
      const data = batch.data;

      // Clear batch data before dispatching so concurrent writes start fresh.
      batch.data = '';
      batch.sessionId = '';

      fireAndForgetInvoke(command, {
        request: {
          sessionId: sid,
          data,
        },
      });
    }

    inputBatches.delete(key);
  };

  if (sessionId) {
    flushOne(sessionId);
  } else {
    for (const key of Array.from(inputBatches.keys())) {
      flushOne(key);
    }
  }
}

/**
 * Invoke a Tauri command and throw on failure
 * Use this when you want exceptions to propagate
 */
export async function invokeOrThrow<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  const result = await safeInvoke<T>(command, args);
  if (!result.success) {
    throw result.error;
  }
  return result.data;
}

/**
 * Invoke a Tauri command with a fallback value on failure
 * Logs the error but returns the fallback instead of throwing
 */
export async function invokeWithFallback<T>(
  command: string,
  fallback: T,
  args?: Record<string, unknown>
): Promise<T> {
  const result = await safeInvoke<T>(command, args);
  if (!result.success) {
    console.warn(`Using fallback for "${command}":`, result.error.message);
    return fallback;
  }
  return result.data;
}
