import { invokeOrThrow } from './tauri';

/** Browser-only previews retain fetch. A native routing failure never falls
 * back to WebView fetch, which would bypass the configured proxy. */
async function nativeRequest(url: string, options: RequestInit, image = false): Promise<{ status: number; body: string }> {
  if (options.signal?.aborted) throw new DOMException('Aborted', 'AbortError');
  const id = crypto.randomUUID();
  const cancel = () => { void invokeOrThrow('network_cancel', { id }).catch(() => {}); };
  options.signal?.addEventListener('abort', cancel, { once: true });
  try {
    const response = await invokeOrThrow<{ status: number; body: string }>('network_request', { request: {
      id, url, image, method: options.method ?? 'GET', headers: Object.fromEntries(new Headers(options.headers)),
      body: typeof options.body === 'string' ? options.body : null,
    } });
    if (options.signal?.aborted) throw new DOMException('Aborted', 'AbortError');
    return response;
  } finally { options.signal?.removeEventListener('abort', cancel); }
}

export async function networkFetch(url: string, options: RequestInit = {}): Promise<Response> {
  if (!('__TAURI_INTERNALS__' in window)) return fetch(url, options);
  const response = await nativeRequest(url, options);
  return new Response(response.status === 204 || response.status === 304 ? null : response.body, { status: response.status });
}

/** Called only after explicit image consent. Packaged apps never put the
 * remote URL into an img element where WebView networking would bypass routing. */
export async function networkImageUrl(url: string, signal: AbortSignal): Promise<string> {
  if (!('__TAURI_INTERNALS__' in window)) return url;
  return (await nativeRequest(url, { signal }, true)).body;
}

export async function checkRoutedUpdate(timeout: number) {
  const { check, Update } = await import('@tauri-apps/plugin-updater');
  if (!('__TAURI_INTERNALS__' in window)) return check({ timeout });
  const metadata = await invokeOrThrow<ConstructorParameters<typeof Update>[0] | null>('network_check_update');
  return metadata ? new Update(metadata) : null;
}
