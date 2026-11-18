import { useEffect, useState } from 'react';
import { safeInvoke } from '../../lib/tauri';
interface ProxyView { mode: 'direct' | 'system' | 'custom'; url: string; username: string; hasPassword: boolean }
export function NetworkProxyControl() {
  const [config, setConfig] = useState<ProxyView>({ mode: 'direct', url: '', username: '', hasPassword: false });
  const [password, setPassword] = useState('');
  const [clearPassword, setClearPassword] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  useEffect(() => {
    let active = true;
    void safeInvoke<ProxyView>('get_network_proxy').then(result => {
      if (!active) return;
      if (result.success) { setConfig(result.data); setLoaded(true); } else setError(result.error.message);
    });
    return () => { active = false; };
  }, []);
  const save = async () => {
    setBusy(true); setError(''); setMessage('');
    try {
      const result = await safeInvoke<ProxyView>('set_network_proxy', { settings: {
        mode: config.mode, url: config.url, username: config.username,
        password: clearPassword ? '' : password || null,
      } });
      if (result.success) { setConfig(result.data); setPassword(''); setClearPassword(false); setMessage('Saved. New connections use this route; reconnect existing sessions to apply it.'); }
      else setError(result.error.message);
    } finally { setBusy(false); }
  };
  const input = 'mt-1 w-full rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-tokyo-fg';
  return <fieldset className="space-y-3" disabled={!loaded || busy}>
    <legend className="sr-only">Outbound proxy</legend>
    <label className="block text-sm text-tokyo-fg">Network route<select className={input} value={config.mode} onChange={e => setConfig({ ...config, mode: e.target.value as ProxyView['mode'] })}>
      <option value="direct">Direct</option><option value="system">System environment</option><option value="custom">Custom proxy</option>
    </select></label>
    {config.mode === 'system' && <p className="text-xs text-tokyo-comment">Uses ALL_PROXY, HTTPS_PROXY, HTTP_PROXY and NO_PROXY from the application/service environment. It does not read OS proxy preferences.</p>}
    {config.mode === 'custom' && <>
      <label className="block text-sm text-tokyo-fg">Proxy URL<input className={input} value={config.url} placeholder="socks5h://127.0.0.1:1080" onChange={e => setConfig({ ...config, url: e.target.value })} autoComplete="off" /></label>
      <p className="text-xs text-tokyo-comment">HTTP, HTTPS, SOCKS4/4a and SOCKS5/5h. The “a” and “h” variants resolve destination names at the proxy. Do not include passwords in the URL.</p>
      <label className="block text-sm text-tokyo-fg">Proxy username<input className={input} value={config.username} onChange={e => setConfig({ ...config, username: e.target.value })} autoComplete="off" /></label>
      <label className="block text-sm text-tokyo-fg">Proxy password<input className={input} type="password" value={password} onChange={e => setPassword(e.target.value)} placeholder={config.hasPassword ? 'Leave blank to keep the saved password' : 'Optional'} autoComplete="new-password" /></label>
      {config.hasPassword && <label className="flex items-center gap-2 text-sm text-tokyo-fg"><input type="checkbox" checked={clearPassword} onChange={e => setClearPassword(e.target.checked)} />Clear stored proxy password</label>}
    </>}
    <p className="text-xs text-tokyo-comment">Applies to native SSH/SFTP connections, database clients, cloud sync, AI prediction and signed updates. A proxy error never falls back to direct access. External coding agents, commands running on remote machines, and your browser use their own network settings.</p>
    <button type="button" onClick={() => void save()} className="rounded-md bg-tokyo-blue px-3 py-2 text-sm text-tokyo-on-accent disabled:opacity-50">Save network route</button>
    {message && <p role="status" className="text-sm text-tokyo-green">{message}</p>}
    {error && <p role="alert" className="text-sm text-tokyo-red">{error}</p>}
  </fieldset>;
}
