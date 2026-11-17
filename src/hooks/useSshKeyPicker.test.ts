import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const safeInvoke = vi.hoisted(() => vi.fn());
vi.mock('../lib/tauri', () => ({ safeInvoke }));
import { useSshKeyPicker } from './useSshKeyPicker';
afterEach(cleanup);
beforeEach(() => { safeInvoke.mockReset(); });
it('ignores a file read completed after reset', async () => {
  let finish!: (value: unknown) => void;
  safeInvoke.mockImplementation((command: string) => command === 'pick_ssh_key_file'
    ? Promise.resolve({ success: true, data: '/temporary/key' })
    : new Promise((resolve) => { finish = resolve; }));
  const { result } = renderHook(() => useSshKeyPicker({ onError: vi.fn() }));
  let read!: Promise<void>;
  act(() => { read = result.current.browseForSshKey(); });
  await waitFor(() => expect(finish).toBeDefined());
  act(() => result.current.reset());
  await act(async () => { finish({ success: true, data: 'stale-key-material' }); await read; });
  expect(result.current.keyContent).toBeNull(); expect(result.current.keyPath).toBeNull();
  expect(result.current.isLoadingKey).toBe(false);
});
it('clears old key material if reading the replacement fails', async () => {
  safeInvoke.mockResolvedValueOnce({ success: true, data: '/new/key' })
    .mockResolvedValueOnce({ success: false, error: { message: 'unreadable' } });
  const onError = vi.fn();
  const { result } = renderHook(() => useSshKeyPicker({ onError }));
  act(() => result.current.setKey('/old/key', 'old-material'));
  await act(() => result.current.browseForSshKey());
  expect(result.current.keyContent).toBeNull(); expect(result.current.keyPath).toBeNull();
  expect(onError).toHaveBeenCalledWith('Failed to read key file: unreadable');
});
it('manual replacement never retains a different key file path', () => {
  const { result } = renderHook(() => useSshKeyPicker({ onError: vi.fn() }));
  act(() => result.current.setKey('/old/key', 'old-material'));
  act(() => result.current.setKeyContent('pasted-material'));
  expect(result.current.keyPath).toBeNull(); expect(result.current.keyContent).toBe('pasted-material');
});
