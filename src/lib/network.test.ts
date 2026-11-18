import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { invokeOrThrow, updaterCheck, updaterResource } = vi.hoisted(() => ({
  invokeOrThrow: vi.fn(), updaterCheck: vi.fn(), updaterResource: vi.fn(),
}));
vi.mock('./tauri', () => ({ invokeOrThrow }));
vi.mock('@tauri-apps/plugin-updater', () => ({
  check: updaterCheck,
  Update: class {
    constructor(metadata: Record<string, unknown>) {
      updaterResource(metadata);
      Object.assign(this, metadata);
    }
  },
}));
import { checkRoutedUpdate, networkFetch, networkImageUrl } from './network';
const browserFetch = vi.fn();
beforeEach(() => {
  invokeOrThrow.mockReset(); browserFetch.mockReset(); updaterCheck.mockReset(); updaterResource.mockReset();
  vi.stubGlobal('fetch', browserFetch);
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

it('preserves JSON API responses and bodyless statuses through the native bridge', async () => {
  invokeOrThrow.mockResolvedValueOnce({ status: 200, body: '{"choices":[{"message":{"content":"suffix"}}]}' });
  const response = await networkFetch('https://model.invalid/chat/completions', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"model":"fixture"}',
  });
  expect(response.ok).toBe(true);
  expect((await response.json()).choices[0].message.content).toBe('suffix');
  expect(invokeOrThrow).toHaveBeenCalledWith('network_request', expect.objectContaining({ request: expect.objectContaining({
    method: 'POST', body: '{"model":"fixture"}', headers: { 'content-type': 'application/json' },
  }) }));
  for (const status of [204, 304]) {
    invokeOrThrow.mockResolvedValueOnce({ status, body: '' });
    expect((await networkFetch('https://example.invalid/')).status).toBe(status);
  }
  expect(browserFetch).not.toHaveBeenCalled();
});

it('keeps the native updater resource rather than opening an unconfigured client', async () => {
  const metadata = { rid: 42, currentVersion: '1.2.0', version: '1.2.1', date: '2026-11-18T00:00:00Z', rawJson: {} };
  invokeOrThrow.mockResolvedValueOnce(metadata);
  expect(await checkRoutedUpdate(12000)).toMatchObject(metadata);
  expect(updaterResource).toHaveBeenCalledWith(metadata);
  expect(updaterCheck).not.toHaveBeenCalled();
  invokeOrThrow.mockResolvedValueOnce(null);
  expect(await checkRoutedUpdate(12000)).toBeNull();
});

it('does not bypass a failed native updater route and retains the browser-preview path', async () => {
  invokeOrThrow.mockRejectedValueOnce(new Error('Configured updater route failed'));
  await expect(checkRoutedUpdate(12000)).rejects.toThrow('Configured updater route failed');
  expect(updaterCheck).not.toHaveBeenCalled();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  updaterCheck.mockResolvedValueOnce(null);
  expect(await checkRoutedUpdate(12000)).toBeNull();
  expect(updaterCheck).toHaveBeenCalledWith({ timeout: 12000 });
});
