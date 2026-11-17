import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { invokeOrThrow } = vi.hoisted(() => ({ invokeOrThrow: vi.fn() }));
vi.mock('./tauri', () => ({ invokeOrThrow }));
import { networkFetch, networkImageUrl } from './network';
const browserFetch = vi.fn();
beforeEach(() => {
  invokeOrThrow.mockReset(); browserFetch.mockReset(); vi.stubGlobal('fetch', browserFetch);
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
});
afterEach(() => { delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__; vi.unstubAllGlobals(); });
it('never falls back to WebView fetch after a native route error', async () => {
  invokeOrThrow.mockRejectedValue(new Error('Configured proxy unavailable'));
  await expect(networkFetch('https://example.invalid/')).rejects.toThrow('Configured proxy');
  expect(browserFetch).not.toHaveBeenCalled();
});
it('routes consented images through bounded native fetch instead of returning a remote image URL', async () => {
  invokeOrThrow.mockResolvedValue({ status: 200, body: 'data:image/png;base64,AAAA' });
  expect(await networkImageUrl('https://example.invalid/image.png', new AbortController().signal)).toBe('data:image/png;base64,AAAA');
  expect(invokeOrThrow).toHaveBeenCalledWith('network_request', expect.objectContaining({ request: expect.objectContaining({ image: true }) }));
  expect(browserFetch).not.toHaveBeenCalled();
});
it('sends cancellation to the native owner and does not return a stale response', async () => {
  const abort = new AbortController();
  let resolve!: (result: {status: number; body: string}) => void;
  invokeOrThrow.mockImplementation((command: string) => command === 'network_request'
    ? new Promise(r => { resolve = r; }) : Promise.resolve());
  const promise = networkFetch('https://example.invalid/', { signal: abort.signal });
  abort.abort(); resolve({ status: 200, body: '{}' });
  await expect(promise).rejects.toThrow('Aborted');
  expect(invokeOrThrow).toHaveBeenCalledWith('network_cancel', expect.objectContaining({ id: expect.any(String) }));
  expect(browserFetch).not.toHaveBeenCalled();
});
