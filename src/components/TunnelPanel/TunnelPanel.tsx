import { useState, useEffect } from 'react';
import { ArrowRightLeft, Plus, Trash2, Play, Square, Globe, ArrowUpRight, ArrowDownLeft, RefreshCw, ChevronDown } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useTunnelStore } from '../../stores/tunnelStore';
import type { TunnelType, TunnelConfigInput } from '../../types/tunnel';

interface TunnelPanelProps {
  serverId: string;
  sessionId?: string;
}

export default function TunnelPanel({ serverId, sessionId }: TunnelPanelProps) {
  const { configs, activeTunnels, fetchConfigs, addConfig, deleteConfig, startTunnel, stopTunnel, fetchActiveTunnels } = useTunnelStore();
  const [showAddForm, setShowAddForm] = useState(false);
  const [newTunnel, setNewTunnel] = useState<TunnelConfigInput>({
    server_id: serverId,
    tunnel_type: 'local',
    local_host: '127.0.0.1',
    local_port: 8080,
    remote_host: 'localhost',
    remote_port: 80,
    auto_start: false,
    enabled: true,
  });

  useEffect(() => {
    fetchConfigs(serverId);
    if (sessionId) fetchActiveTunnels(sessionId);
  }, [serverId, sessionId]);

  const handleAdd = async () => {
    try {
      await addConfig({ ...newTunnel, server_id: serverId });
      setShowAddForm(false);
      setNewTunnel(prev => ({ ...prev, local_port: prev.local_port + 1 }));
    } catch { /* error handled in store */ }
  };

  const handleStart = async (config: typeof configs[0]) => {
    if (!sessionId) return;
    try {
      await startTunnel(sessionId, {
        server_id: config.server_id,
        tunnel_type: config.tunnel_type,
        local_host: config.local_host,
        local_port: config.local_port,
        remote_host: config.remote_host,
        remote_port: config.remote_port,
        auto_start: config.auto_start,
        enabled: config.enabled,
      });
    } catch { /* error handled */ }
  };

  const isActive = (configId: string) => activeTunnels.some(t => t.config.id === configId);

  const tunnelTypeIcon = (type_: TunnelType) => {
    switch (type_) {
      case 'local': return <ArrowDownLeft className="w-3.5 h-3.5" />;
      case 'remote': return <ArrowUpRight className="w-3.5 h-3.5" />;
      case 'dynamic': return <Globe className="w-3.5 h-3.5" />;
    }
  };

  const tunnelTypeLabel = (type_: TunnelType) => {
    switch (type_) {
      case 'local': return 'Local (-L)';
      case 'remote': return 'Remote (-R)';
      case 'dynamic': return 'Dynamic (-D)';
    }
  };

  return (
    <div className="flex flex-col h-full bg-tokyo-bg text-tokyo-fg">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
        <div className="flex items-center gap-2">
          <ArrowRightLeft className="w-4 h-4 text-tokyo-blue" />
          <span className="text-sm font-semibold text-white">SSH Tunnels</span>
          <span className="text-xs text-tokyo-comment px-1.5 py-0.5 rounded-full bg-tokyo-bg-hl">
            {configs.length} configs &middot; {activeTunnels.length} active
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => { if (sessionId) fetchActiveTunnels(sessionId); }}
            className="p-1.5 rounded-md hover:bg-tokyo-bg-hl transition-colors cursor-pointer text-tokyo-comment hover:text-tokyo-fg"
            title="Refresh"
          >
            <RefreshCw className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className={cn(
              'p-1.5 rounded-md transition-colors cursor-pointer',
              showAddForm
                ? 'bg-tokyo-blue/20 text-tokyo-blue'
                : 'hover:bg-tokyo-bg-hl text-tokyo-green hover:text-tokyo-green'
            )}
            title="Add Tunnel Config"
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Add Form */}
      {showAddForm && (
        <div className="p-3 border-b border-tokyo-bg-hl bg-tokyo-bg-dark space-y-3">
          <div>
            <label className="block text-xs text-tokyo-comment mb-1 font-medium">Tunnel Type</label>
            <div className="relative">
              <select
                value={newTunnel.tunnel_type}
                onChange={e => setNewTunnel(prev => ({ ...prev, tunnel_type: e.target.value as TunnelType }))}
                className="w-full bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm text-tokyo-fg cursor-pointer
                           focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue appearance-none"
              >
                <option value="local">Local Forward (-L)</option>
                <option value="remote">Remote Forward (-R)</option>
                <option value="dynamic">Dynamic SOCKS5 (-D)</option>
              </select>
              <ChevronDown className="absolute right-2 top-1/2 -translate-y-1/2 w-4 h-4 text-tokyo-comment pointer-events-none" />
            </div>
          </div>
          <div>
            <label className="block text-xs text-tokyo-comment mb-1 font-medium">Local Bind</label>
            <div className="flex gap-2 items-center">
              <input
                type="text"
                placeholder="Host"
                value={newTunnel.local_host}
                onChange={e => setNewTunnel(prev => ({ ...prev, local_host: e.target.value }))}
                className="bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm flex-1 text-tokyo-fg
                           focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue placeholder:text-tokyo-comment"
              />
              <span className="text-xs text-tokyo-comment font-bold">:</span>
              <input
                type="number"
                placeholder="Port"
                value={newTunnel.local_port}
                onChange={e => setNewTunnel(prev => ({ ...prev, local_port: parseInt(e.target.value) || 0 }))}
                className="bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm w-24 text-tokyo-fg font-mono
                           focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue"
              />
            </div>
          </div>
          {newTunnel.tunnel_type !== 'dynamic' && (
            <div>
              <label className="block text-xs text-tokyo-comment mb-1 font-medium">Remote Target</label>
              <div className="flex gap-2 items-center">
                <input
                  type="text"
                  placeholder="Host"
                  value={newTunnel.remote_host || ''}
                  onChange={e => setNewTunnel(prev => ({ ...prev, remote_host: e.target.value }))}
                  className="bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm flex-1 text-tokyo-fg
                             focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue placeholder:text-tokyo-comment"
                />
                <span className="text-xs text-tokyo-comment font-bold">:</span>
                <input
                  type="number"
                  placeholder="Port"
                  value={newTunnel.remote_port || ''}
                  onChange={e => setNewTunnel(prev => ({ ...prev, remote_port: parseInt(e.target.value) || undefined }))}
                  className="bg-tokyo-bg border border-tokyo-bg-hl rounded-md px-3 py-2 text-sm w-24 text-tokyo-fg font-mono
                             focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:border-tokyo-blue"
                />
              </div>
            </div>
          )}
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-tokyo-fg cursor-pointer">
              <input
                type="checkbox"
                checked={newTunnel.auto_start}
                onChange={e => setNewTunnel(prev => ({ ...prev, auto_start: e.target.checked }))}
                className="rounded border-tokyo-bg-hl bg-tokyo-bg accent-tokyo-blue cursor-pointer"
              />
              Auto-start on connect
            </label>
          </div>
          <div className="flex justify-end gap-2 pt-1">
            <button
              onClick={() => setShowAddForm(false)}
              className="px-3 py-1.5 text-sm rounded-md bg-tokyo-bg-hl text-tokyo-fg hover:bg-tokyo-selection hover:text-white transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              onClick={handleAdd}
              className="px-3 py-1.5 text-sm rounded-md bg-tokyo-blue text-white hover:bg-tokyo-blue/80 transition-colors cursor-pointer"
            >
              Save Config
            </button>
          </div>
        </div>
      )}

      {/* Tunnel List */}
      <div className="flex-1 overflow-y-auto">
        {configs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-tokyo-comment py-12">
            <ArrowRightLeft className="w-10 h-10 mb-3 opacity-40" />
            <p className="text-sm font-medium">No tunnel configurations</p>
            <p className="text-xs mt-1">Click <span className="text-tokyo-green">+</span> to add a port forward</p>
          </div>
        ) : (
          <div className="space-y-1.5 p-3">
            {configs.map(config => {
              const active = isActive(config.id);
              const activeTunnel = activeTunnels.find(t => t.config.id === config.id);
              return (
                <div
                  key={config.id}
                  className={cn(
                    'flex items-center gap-3 px-3 py-2.5 rounded-lg text-xs transition-all duration-200',
                    active
                      ? 'bg-tokyo-green/5 border border-tokyo-green/20 shadow-sm shadow-tokyo-green/5'
                      : 'bg-tokyo-bg-dark border border-tokyo-bg-hl hover:border-tokyo-selection'
                  )}
                >
                  <span className={active ? 'text-tokyo-green' : 'text-tokyo-comment'}>
                    {tunnelTypeIcon(config.tunnel_type)}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-1.5">
                      <span className="font-medium text-white">{tunnelTypeLabel(config.tunnel_type)}</span>
                      {active && <span className="w-1.5 h-1.5 rounded-full bg-tokyo-green animate-pulse" />}
                    </div>
                    <div className="text-tokyo-comment truncate mt-0.5 font-mono">
                      {config.local_host}:{config.local_port}
                      {config.tunnel_type !== 'dynamic' && (
                        <span className="text-tokyo-fg/40"> &rarr; </span>
                      )}
                      {config.tunnel_type !== 'dynamic' && `${config.remote_host}:${config.remote_port}`}
                    </div>
                    {activeTunnel && (
                      <div className="text-tokyo-comment mt-0.5">
                        <span className="text-tokyo-cyan">&uarr;{formatBytes(activeTunnel.bytesIn)}</span>
                        {' '}<span className="text-tokyo-magenta">&darr;{formatBytes(activeTunnel.bytesOut)}</span>
                        {' '}&middot; {activeTunnel.activeConnections} conn
                      </div>
                    )}
                  </div>
                  <div className="flex items-center gap-1">
                    {sessionId && !active && (
                      <button
                        onClick={() => handleStart(config)}
                        className="p-1.5 rounded-md hover:bg-tokyo-green/10 text-tokyo-green transition-colors cursor-pointer"
                        title="Start tunnel"
                      >
                        <Play className="w-3.5 h-3.5" />
                      </button>
                    )}
                    {active && activeTunnel && (
                      <button
                        onClick={() => stopTunnel(activeTunnel.id)}
                        className="p-1.5 rounded-md hover:bg-tokyo-red/10 text-tokyo-red transition-colors cursor-pointer"
                        title="Stop tunnel"
                      >
                        <Square className="w-3.5 h-3.5" />
                      </button>
                    )}
                    <button
                      onClick={() => deleteConfig(config.id)}
                      className="p-1.5 rounded-md hover:bg-tokyo-red/10 text-tokyo-comment hover:text-tokyo-red transition-colors cursor-pointer"
                      title="Delete config"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)}K`;
  return `${(bytes / 1048576).toFixed(1)}M`;
}
