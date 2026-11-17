import { useCallback, useEffect, useRef, useState } from 'react';
import { safeInvoke } from '../lib/tauri';

export interface UseSshKeyPickerOptions {
  /** Called when the native picker opens; dialogs use it to clear a prior error. */
  onBrowseStart?: () => void;
  /** Called with a user-facing message when picking or reading the key fails. */
  onError: (message: string) => void;
}

export interface SshKeyPicker {
  /** Path of the picked (or restored) key file, if any. */
  keyPath: string | null;
  /** Key contents: picked from disk, pasted, or restored from saved credentials. */
  keyContent: string | null;
  /** True while the native picker / file read is in flight. */
  isLoadingKey: boolean;
  /** Open the native picker and read the chosen key file. */
  browseForSshKey: () => Promise<void>;
  /** Replace the key content directly (e.g. a pasted key on mobile). */
  setKeyContent: (content: string | null) => void;
  /** Restore a previously saved key path + content. */
  setKey: (path: string | null, content: string | null) => void;
  /** Reset all picker state to its initial values. */
  reset: () => void;
}

/**
 * SSH private key file picker flow (`pick_ssh_key_file` + `read_ssh_key_file`)
 * with its loading state, shared by the Add Server and Connect dialogs.
 * Errors are reported through `onError` so each dialog keeps its own error slot.
 */
export function useSshKeyPicker({ onBrowseStart, onError }: UseSshKeyPickerOptions): SshKeyPicker {
  const [keyPath, setKeyPath] = useState<string | null>(null);
  const [keyContent, setKeyContent] = useState<string | null>(null);
  const [isLoadingKey, setIsLoadingKey] = useState(false);
  const generation = useRef(0);
  useEffect(() => () => { generation.current += 1; }, []);

  // Keep the latest callbacks without making browseForSshKey unstable.
  const callbacksRef = useRef({ onBrowseStart, onError });
  callbacksRef.current = { onBrowseStart, onError };

  const browseForSshKey = useCallback(async () => {
    const request = ++generation.current;
    setIsLoadingKey(true);
    callbacksRef.current.onBrowseStart?.();

    try {
      const result = await safeInvoke<string | null>('pick_ssh_key_file');
      if (request !== generation.current) return;
      if (!result.success) { callbacksRef.current.onError(result.error.message); return; }

      if (result.success && result.data) {
        const path = result.data;
        setKeyPath(path);
        setKeyContent(null);

        // Read the key file content
        const readResult = await safeInvoke<string>('read_ssh_key_file', { path });
        if (request !== generation.current) return;

        if (readResult.success) {
          setKeyContent(readResult.data);
        } else {
          callbacksRef.current.onError(`Failed to read key file: ${readResult.error.message}`);
          setKeyPath(null);
        }
      }
    } catch (err) {
      if (request !== generation.current) return;
      setKeyPath(null);
      setKeyContent(null);
      callbacksRef.current.onError(err instanceof Error ? err.message : 'Failed to browse for key file');
    } finally {
      if (request === generation.current) setIsLoadingKey(false);
    }
  }, []);

  const setKey = useCallback((path: string | null, content: string | null) => {
    generation.current += 1;
    setIsLoadingKey(false);
    setKeyPath(path);
    setKeyContent(content);
  }, []);

  const reset = useCallback(() => {
    generation.current += 1;
    setKeyPath(null);
    setKeyContent(null);
    setIsLoadingKey(false);
  }, []);

  const pasteKeyContent = useCallback((content: string | null) => {
    generation.current += 1;
    setIsLoadingKey(false);
    setKeyPath(null);
    setKeyContent(content);
  }, []);

  return { keyPath, keyContent, isLoadingKey, browseForSshKey, setKeyContent: pasteKeyContent, setKey, reset };
}

/** Basename of a key path for display, tolerating both path separators. */
export function getFileName(path: string | null): string {
  if (!path) return '';
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}
