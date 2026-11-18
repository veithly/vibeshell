import { useEffect, useState } from 'react';
import { safeInvoke } from '../../lib/tauri';

export function GatewayPortControl() {
  const [port, setPort] = useState('0');
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    void safeInvoke<{ port: number }>('get_gateway_port').then(result => {
      if (!active) return;
      if (result.success) { setPort(String(result.data.port)); setLoaded(true); }
      else setError(result.error.message);
    });
    return () => { active = false; };
  }, []);
  const save = async () => {
    const value = Number(port);
    setMessage(''); setError('');
    if (!/^\d+$/.test(port) || !Number.isInteger(value) || value < 0 || value > 65535) {
      setError('Enter 0 for automatic allocation, or a fixed port from 1 to 65535.'); return;
    }
    setSaving(true);
    try {
      const result = await safeInvoke('set_gateway_port', { settings: { port: value } });
      if (result.success) setMessage('Saved. Restart VibeShell after saving your work to apply this port.');
      else setError(result.error.message);
    } finally { setSaving(false); }
  };
  return <div className="space-y-2 border-t border-tokyo-bg-hl p-4">
    <label htmlFor="gateway-port" className="block text-sm text-tokyo-fg">Gateway port</label>
    <div className="flex flex-wrap items-center gap-2">
      <input id="gateway-port" type="number" min="0" max="65535" step="1" value={port}
        disabled={!loaded || saving} onChange={e => { setPort(e.target.value); setMessage(''); }}
        aria-describedby="gateway-port-help" className="w-32 rounded-md border border-tokyo-bg-hl bg-tokyo-bg px-3 py-2 text-tokyo-fg" />
      <button type="button" disabled={!loaded || saving} onClick={() => void save()}
        className="rounded-md bg-tokyo-blue px-3 py-2 text-sm text-tokyo-on-accent disabled:opacity-50">Save port</button>
    </div>
    <p id="gateway-port-help" className="text-xs text-tokyo-comment">0 selects an available port automatically. A fixed port persists across restarts. The gateway remains local-only and requires authentication; a busy port causes an error, not a fallback.</p>
    {message && <p role="status" className="text-sm text-tokyo-green">{message}</p>}
    {error && <p role="alert" className="text-sm text-tokyo-red">{error}</p>}
  </div>;
}
