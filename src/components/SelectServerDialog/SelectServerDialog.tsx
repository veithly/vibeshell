import { useCallback, useMemo, useEffect, useState } from 'react';
import { X, Server, Plus, Wifi, FolderOpen, Terminal, Monitor } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore, useServersWithGroups } from '../../stores/serverStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useLocalShellStore, useAvailableShells } from '../../stores/localShellStore';
import type { Server as ServerType } from '../../stores/serverStore';
import type { ShellInfo } from '../../stores/localShellStore';

/** Tab options for the dialog */
type TabOption = 'local' | 'ssh';

interface SelectServerDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onSelectServer: (server: ServerType) => void;
  onSelectLocalShell?: (shell: ShellInfo) => void;
  onAddServer: () => void;
  /** Set of server IDs that are currently connected */
  connectedServerIds?: Set<string>;
}

export function SelectServerDialog({
  isOpen,
  onClose,
  onSelectServer,
  onSelectLocalShell,
  onAddServer,
  connectedServerIds = new Set(),
}: SelectServerDialogProps) {
  const { servers } = useServerStore();
  const { groupedServers, ungroupedServers } = useServersWithGroups();
  const sessions = useSessionStore((state) => state.sessions);
  const createLocalShellSession = useSessionStore((state) => state.createLocalShellSession);
  const setActiveSession = useSessionStore((state) => state.setActiveSession);

  // Local shell state
  const fetchAvailableShells = useLocalShellStore((state) => state.fetchAvailableShells);
  const setLastSelectedShell = useLocalShellStore((state) => state.setLastSelectedShell);
  const { shells, suggestedShell } = useAvailableShells();

  // Tab state - remember last selected
  const [activeTab, setActiveTab] = useState<TabOption>(() => {
    const saved = localStorage.getItem('newConnectionTab');
    return (saved === 'local' || saved === 'ssh') ? saved : 'local';
  });

  // Fetch available shells when dialog opens
  useEffect(() => {
    if (isOpen) {
      fetchAvailableShells();
    }
  }, [isOpen, fetchAvailableShells]);

  // Save tab preference
  const handleTabChange = useCallback((tab: TabOption) => {
    setActiveTab(tab);
    localStorage.setItem('newConnectionTab', tab);
  }, []);

  // Compute session counts per server (only count active sessions)
  const sessionCounts = useMemo(() => {
    const counts = new Map<string, number>();
    sessions.forEach((session) => {
      if ((session.state === 'connected' || session.state === 'connecting') && session.sessionType === 'ssh') {
        const current = counts.get(session.serverId) || 0;
        counts.set(session.serverId, current + 1);
      }
    });
    return counts;
  }, [sessions]);

  // Count local shell sessions
  const localShellCount = useMemo(() => {
    return sessions.filter((s) =>
      s.sessionType === 'local' && (s.state === 'connected' || s.state === 'connecting')
    ).length;
  }, [sessions]);

  const handleServerClick = useCallback((server: ServerType) => {
    onSelectServer(server);
    onClose();
  }, [onSelectServer, onClose]);

  const handleLocalShellClick = useCallback(async (shell: ShellInfo) => {
    // Remember this shell preference
    setLastSelectedShell(shell.id);

    if (onSelectLocalShell) {
      onSelectLocalShell(shell);
      onClose();
    } else {
      // Create local shell session directly
      const session = await createLocalShellSession(shell.id, 80, 24);
      if (session) {
        setActiveSession(session.id);
        onClose();
      }
    }
  }, [onSelectLocalShell, onClose, createLocalShellSession, setActiveSession, setLastSelectedShell]);

  const handleAddServer = useCallback(() => {
    onClose();
    onAddServer();
  }, [onClose, onAddServer]);

  if (!isOpen) return null;

  const hasServers = servers.length > 0;
  const hasShells = shells.length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />

      {/* Dialog */}
      <div className="relative bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl w-full max-w-md mx-4 max-h-[70vh] overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-tokyo-bg-hl">
          <div className="flex items-center gap-2">
            <Terminal className="w-5 h-5 text-tokyo-blue" />
            <h2 className="text-lg font-semibold text-white">New Connection</h2>
          </div>
          <button
            className="p-1 rounded-md text-tokyo-comment hover:text-white hover:bg-tokyo-bg-hl transition-colors"
            onClick={onClose}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex border-b border-tokyo-bg-hl">
          <button
            className={cn(
              'flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors',
              activeTab === 'local'
                ? 'text-tokyo-blue border-b-2 border-tokyo-blue bg-tokyo-bg-hl/30'
                : 'text-tokyo-comment hover:text-tokyo-fg'
            )}
            onClick={() => handleTabChange('local')}
          >
            <Monitor className="w-4 h-4" />
            <span>Local Shell</span>
            {localShellCount > 0 && (
              <span className="text-[10px] bg-tokyo-blue/20 text-tokyo-blue px-1.5 py-0.5 rounded-full">
                {localShellCount}
              </span>
            )}
          </button>
          <button
            className={cn(
              'flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium transition-colors',
              activeTab === 'ssh'
                ? 'text-tokyo-blue border-b-2 border-tokyo-blue bg-tokyo-bg-hl/30'
                : 'text-tokyo-comment hover:text-tokyo-fg'
            )}
            onClick={() => handleTabChange('ssh')}
          >
            <Server className="w-4 h-4" />
            <span>SSH Server</span>
            {connectedServerIds.size > 0 && (
              <span className="text-[10px] bg-tokyo-green/20 text-tokyo-green px-1.5 py-0.5 rounded-full">
                {connectedServerIds.size}
              </span>
            )}
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-4">
          {activeTab === 'local' ? (
            // Local Shell Content
            hasShells ? (
              <div className="space-y-4">
                <p className="text-sm text-tokyo-comment">
                  Select a shell to open a local terminal:
                </p>

                {/* Suggested Shell */}
                {suggestedShell && (
                  <div className="space-y-2">
                    <div className="text-xs font-medium text-tokyo-comment uppercase tracking-wider">
                      Suggested
                    </div>
                    <ShellItem
                      shell={suggestedShell}
                      isDefault={suggestedShell.isDefault}
                      onClick={() => handleLocalShellClick(suggestedShell)}
                    />
                  </div>
                )}

                {/* All Shells */}
                <div className="space-y-2">
                  <div className="text-xs font-medium text-tokyo-comment uppercase tracking-wider">
                    Available Shells
                  </div>
                  <div className="space-y-1">
                    {shells
                      .filter((s) => s.id !== suggestedShell?.id)
                      .map((shell) => (
                        <ShellItem
                          key={shell.id}
                          shell={shell}
                          isDefault={shell.isDefault}
                          onClick={() => handleLocalShellClick(shell)}
                        />
                      ))}
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-center py-8">
                <div className="text-4xl mb-4 text-tokyo-comment">
                  <Terminal className="w-12 h-12 mx-auto opacity-50" />
                </div>
                <p className="text-tokyo-fg mb-2">No shells detected</p>
                <p className="text-tokyo-comment text-sm">
                  Unable to detect any shells on this system.
                </p>
              </div>
            )
          ) : (
            // SSH Server Content
            hasServers ? (
              <div className="space-y-4">
                <p className="text-sm text-tokyo-comment">
                  Select a server to connect to:
                </p>

                {/* Grouped Servers */}
                {groupedServers.map(({ group, servers: groupServers }) => (
                  groupServers.length > 0 && (
                    <div key={group.id} className="space-y-2">
                      <div className="flex items-center gap-2 text-xs font-medium text-tokyo-comment uppercase tracking-wider">
                        <FolderOpen className="w-3.5 h-3.5" style={{ color: group.color }} />
                        <span>{group.name}</span>
                      </div>
                      <div className="space-y-1">
                        {groupServers.map((server) => (
                          <ServerItem
                            key={server.id}
                            server={server}
                            isConnected={connectedServerIds.has(server.id)}
                            sessionCount={sessionCounts.get(server.id) || 0}
                            onClick={() => handleServerClick(server)}
                          />
                        ))}
                      </div>
                    </div>
                  )
                ))}

                {/* Ungrouped Servers */}
                {ungroupedServers.length > 0 && (
                  <div className="space-y-2">
                    {groupedServers.some(g => g.servers.length > 0) && (
                      <div className="text-xs font-medium text-tokyo-comment uppercase tracking-wider">
                        Ungrouped
                      </div>
                    )}
                    <div className="space-y-1">
                      {ungroupedServers.map((server) => (
                        <ServerItem
                          key={server.id}
                          server={server}
                          isConnected={connectedServerIds.has(server.id)}
                          sessionCount={sessionCounts.get(server.id) || 0}
                          onClick={() => handleServerClick(server)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-center py-8">
                <div className="text-4xl mb-4 text-tokyo-comment">
                  <Server className="w-12 h-12 mx-auto opacity-50" />
                </div>
                <p className="text-tokyo-fg mb-2">No servers configured</p>
                <p className="text-tokyo-comment text-sm mb-6">
                  Add a server to get started with SSH connections.
                </p>
              </div>
            )
          )}
        </div>

        {/* Footer */}
        {activeTab === 'ssh' && (
          <div className="border-t border-tokyo-bg-hl px-4 py-3">
            <button
              onClick={handleAddServer}
              className={cn(
                'w-full flex items-center justify-center gap-2 px-4 py-2 rounded-md',
                'bg-tokyo-blue text-white',
                'hover:bg-tokyo-blue/80',
                'transition-colors'
              )}
            >
              <Plus className="w-4 h-4" />
              <span>Add New Server</span>
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

interface ShellItemProps {
  shell: ShellInfo;
  isDefault: boolean;
  onClick: () => void;
}

function ShellItem({ shell, isDefault, onClick }: ShellItemProps) {
  // Get icon based on shell type
  const getShellIcon = (shellId: string) => {
    if (shellId.includes('powershell') || shellId === 'pwsh') {
      return <span className="text-[10px] font-bold text-tokyo-blue">PS</span>;
    }
    if (shellId === 'cmd') {
      return <span className="text-[10px] font-bold text-tokyo-yellow">CMD</span>;
    }
    if (shellId.includes('bash') || shellId === 'wsl' || shellId === 'zsh' || shellId === 'fish' || shellId === 'sh') {
      return <span className="text-[10px] font-bold text-tokyo-green">$</span>;
    }
    return <Terminal className="w-4 h-4 text-tokyo-comment" />;
  };

  return (
    <button
      onClick={onClick}
      className={cn(
        'w-full flex items-center gap-3 px-3 py-2.5 rounded-md text-left',
        'bg-tokyo-bg border border-tokyo-bg-hl',
        'hover:bg-tokyo-bg-hl hover:border-tokyo-blue',
        'transition-colors duration-150',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
      )}
    >
      <div className="flex-shrink-0 w-6 h-6 flex items-center justify-center rounded bg-tokyo-bg-hl">
        {getShellIcon(shell.id)}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-tokyo-fg truncate">
          {shell.name}
        </div>
        <div className="text-xs text-tokyo-comment truncate">
          {shell.path}
        </div>
      </div>
      {isDefault && (
        <span className="text-[10px] bg-tokyo-blue/20 text-tokyo-blue px-1.5 py-0.5 rounded-full font-medium">
          Default
        </span>
      )}
    </button>
  );
}

interface ServerItemProps {
  server: ServerType;
  isConnected: boolean;
  sessionCount?: number;
  onClick: () => void;
}

function ServerItem({ server, isConnected, sessionCount = 0, onClick }: ServerItemProps) {
  return (
    <button
      onClick={onClick}
      className={cn(
        'w-full flex items-center gap-3 px-3 py-2.5 rounded-md text-left',
        'bg-tokyo-bg border border-tokyo-bg-hl',
        'hover:bg-tokyo-bg-hl hover:border-tokyo-blue',
        'transition-colors duration-150',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
      )}
    >
      <div className="flex-shrink-0">
        {isConnected ? (
          <Wifi className="w-4 h-4 text-tokyo-green" />
        ) : (
          <Server className="w-4 h-4 text-tokyo-comment" />
        )}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-tokyo-fg truncate">
          {server.name}
        </div>
        <div className="text-xs text-tokyo-comment truncate">
          {server.username}@{server.host}:{server.port}
        </div>
      </div>
      {isConnected && (
        <div className="flex items-center gap-2">
          {sessionCount > 0 && (
            <span className="text-[10px] bg-tokyo-green/20 text-tokyo-green px-1.5 py-0.5 rounded-full font-medium">
              {sessionCount} {sessionCount === 1 ? 'session' : 'sessions'}
            </span>
          )}
          <span className="text-xs text-tokyo-green">+New</span>
        </div>
      )}
    </button>
  );
}

export type { SelectServerDialogProps };
