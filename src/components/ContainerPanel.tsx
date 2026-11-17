import { useCallback, useEffect, useState } from 'react';
import { safeInvoke } from '../lib/tauri';
import { useSessionStore, type SessionInfo } from '../stores/sessionStore';
import { X, RefreshCw } from 'lucide-react';

interface ContainerInfo { ID: string; Names: string; Image: string; State: string; Status: string }
export function ContainerPanel({ parentId, onClose, onConnected }: { parentId: string; onClose: () => void; onConnected: (id: string) => void }) {
  const [items, setItems] = useState<ContainerInfo[]>([]);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [opening, setOpening] = useState(false);
  const [user, setUser] = useState('');
  const [workdir, setWorkdir] = useState('');
  const [shell, setShell] = useState('/bin/sh');
  const refresh = useCallback(async () => {
    setLoading(true); setError('');
    try {
      const result = await safeInvoke<ContainerInfo[]>('container_list', { parentSessionId: parentId });
      if (result.success) setItems(result.data); else setError(result.error.message);
    } finally { setLoading(false); }
  }, [parentId]);
  useEffect(() => { void refresh(); }, [refresh]);
  const open = async (container: string) => {
    setOpening(true); setError('');
    try {
      const result = await safeInvoke<SessionInfo>('container_open', { request: {
        parent_session_id: parentId, container, user: user || null, workdir: workdir || null, shell,
      } });
      if (!result.success) { setError(result.error.message); return; }
      const s = result.data;
      useSessionStore.getState().addSession({ id: s.id, serverId: s.server_id, serverName: s.server_name,
        state: s.state, createdAt: s.created_at * 1000, sessionType: 'ssh', container: s.container });
      onConnected(s.id);
      onClose();
    } finally { setOpening(false); }
  };
  const running = items.filter(item => item.State === 'running');
  const stopped = items.filter(item => item.State !== 'running');
  return <aside aria-label="Docker containers" className="glass-panel w-80 max-w-[90vw] shrink-0 overflow-y-auto border-l border-tokyo-bg-hl p-4">
    <div className="mb-3 flex items-center justify-between"><h2 className="font-medium text-tokyo-fg">Docker containers</h2>
      <div className="flex"><button aria-label="Refresh containers" className="icon-button" disabled={loading || opening} onClick={() => void refresh()}><RefreshCw className="h-4 w-4" /></button><button aria-label="Close containers" className="icon-button" onClick={onClose}><X className="h-4 w-4" /></button></div></div>
    <p className="mb-3 text-xs text-tokyo-comment">Open a separate terminal inside a running container. Closing the terminal leaves the container and host session running.</p>
    <div className="mb-3 space-y-2 text-xs text-tokyo-fg">
      <label className="block">Container user (optional)<input className="mt-1 w-full rounded border border-tokyo-bg-hl bg-tokyo-bg px-2 py-1" value={user} onChange={e => setUser(e.target.value)} /></label>
      <label className="block">Working directory (optional)<input className="mt-1 w-full rounded border border-tokyo-bg-hl bg-tokyo-bg px-2 py-1" value={workdir} onChange={e => setWorkdir(e.target.value)} /></label>
      <label className="block">Shell<select className="ml-2 rounded border border-tokyo-bg-hl bg-tokyo-bg p-1" value={shell} onChange={e => setShell(e.target.value)}>{['/bin/sh', '/bin/bash', '/bin/ash', '/bin/zsh'].map(s => <option key={s}>{s}</option>)}</select></label>
    </div>
    {error && <p role="alert" className="mb-2 text-sm text-tokyo-red">{error}</p>}
    {loading && <p role="status" className="text-sm text-tokyo-comment">Loading containers…</p>}
    <h3 className="text-sm text-tokyo-comment">Running ({running.length})</h3>
    {running.map(item => <button key={item.ID} disabled={opening} onClick={() => void open(item.ID)} className="mt-2 block w-full rounded-lg border border-tokyo-bg-hl p-3 text-left text-tokyo-fg hover:bg-tokyo-bg-hl disabled:opacity-50"><span className="block text-sm">{item.Names}</span><span className="block truncate text-xs text-tokyo-comment">{item.Image} · {item.ID.slice(0, 12)}</span></button>)}
    {!loading && !running.length && <p className="py-3 text-sm text-tokyo-comment">No running containers.</p>}
    <details className="mt-4 text-sm text-tokyo-comment"><summary>Stopped / other ({stopped.length})</summary>{stopped.map(item => <div key={item.ID} className="py-2"><span className="text-tokyo-fg">{item.Names}</span><p className="text-xs">{item.Status} — not started automatically</p></div>)}</details>
  </aside>;
}
