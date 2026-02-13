import { useEffect, useMemo, useState, useCallback } from 'react';
import { ChevronDown, ChevronRight, Loader2, Plus, ServerIcon, FolderClosed, Plug, Edit, Trash2, PlusCircle } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useServerStore, useServersWithGroups, type Server, type Group } from '../../stores/serverStore';
import { useSessionStore } from '../../stores/sessionStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { ServerItem } from './ServerItem';
import { ContextMenu, type ContextMenuItem } from '../ContextMenu';
import { ConfirmDialog } from '../ConfirmDialog';

interface ServerListProps {
  /** Callback when a server is clicked to connect */
  onConnect?: (server: Server) => void;
  /** Callback when "Add Server" button is clicked */
  onAddServer?: () => void;
  /** Callback when Edit Server is requested */
  onEditServer?: (server: Server) => void;
  /** Set of server IDs that have active connections */
  connectedServerIds?: Set<string>;
  /** Callback when "New Session" is requested for an already connected server */
  onNewSession?: (server: Server) => void;
}

interface GroupSectionProps {
  group: Group;
  servers: Server[];
  isExpanded: boolean;
  onToggle: () => void;
  selectedServerId: string | null;
  connectedServerIds: Set<string>;
  /** Map of server ID to session count */
  sessionCounts: Map<string, number>;
  onServerClick: (server: Server) => void;
  onServerContextMenu: (server: Server, event: React.MouseEvent) => void;
}

/**
 * Collapsible group section for organizing servers
 */
function GroupSection({
  group,
  servers,
  isExpanded,
  onToggle,
  selectedServerId,
  connectedServerIds,
  sessionCounts,
  onServerClick,
  onServerContextMenu,
}: GroupSectionProps) {
  if (servers.length === 0) {
    return null;
  }

  return (
    <div className="mb-2">
      {/* Group Header */}
      <button
        className={cn(
          'w-full flex items-center gap-2 px-2 py-1.5 rounded-md',
          'text-xs font-medium text-gray-400 uppercase tracking-wider',
          'hover:bg-gray-700/30 transition-colors duration-150'
        )}
        onClick={onToggle}
        aria-expanded={isExpanded}
      >
        {isExpanded ? (
          <ChevronDown className="w-3 h-3" />
        ) : (
          <ChevronRight className="w-3 h-3" />
        )}
        <FolderClosed
          className="w-3 h-3"
          style={{ color: group.color || '#6b7280' }}
        />
        <span className="truncate">{group.name}</span>
        <span className="ml-auto text-gray-500">{servers.length}</span>
      </button>

      {/* Server Items */}
      {isExpanded && (
        <div className="mt-1 space-y-0.5 pl-2">
          {servers.map((server) => (
            <ServerItem
              key={server.id}
              server={server}
              isSelected={selectedServerId === server.id}
              isConnected={connectedServerIds.has(server.id)}
              sessionCount={sessionCounts.get(server.id) || 0}
              onClick={onServerClick}
              onContextMenu={onServerContextMenu}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Server list component displaying all servers organized by groups
 * Integrates with Zustand store for state management
 */
export function ServerList({
  onConnect,
  onAddServer,
  onEditServer,
  connectedServerIds = new Set(),
  onNewSession,
}: ServerListProps) {
  const {
    loading,
    error,
    selectedServerId,
    fetchServers,
    fetchGroups,
    selectServer,
    deleteServer,
    clearError,
  } = useServerStore();

  // Get sessions to compute session counts per server
  const sessions = useSessionStore((state) => state.sessions);

  // Compute session counts per server (only count active sessions)
  const sessionCounts = useMemo(() => {
    const counts = new Map<string, number>();
    sessions.forEach((session) => {
      if (session.state === 'connected' || session.state === 'connecting') {
        const current = counts.get(session.serverId) || 0;
        counts.set(session.serverId, current + 1);
      }
    });
    return counts;
  }, [sessions]);

  const { success: notifySuccess, error: notifyError } = useNotificationStore();

  const { groupedServers, ungroupedServers } = useServersWithGroups();

  // Get groups array from store (primitive selector, safe for equality check)
  const groups = useServerStore((state) => state.groups);

  // Track expanded state for each group - use useMemo to avoid infinite re-renders
  // Creating new Set in a Zustand selector causes infinite loop due to reference inequality
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  // Initialize expanded groups when groups change
  useEffect(() => {
    setExpandedGroups(new Set(groups.map((g) => g.id)));
  }, [groups]);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    isOpen: boolean;
    position: { x: number; y: number };
    server: Server | null;
  }>({
    isOpen: false,
    position: { x: 0, y: 0 },
    server: null,
  });

  // Delete confirmation state
  const [deleteConfirm, setDeleteConfirm] = useState<Server | null>(null);

  // Fetch servers and groups on mount
  useEffect(() => {
    const loadData = async () => {
      try {
        await Promise.all([fetchServers(), fetchGroups()]);
      } catch {
        // Errors are handled by the store
      }
    };
    loadData();
  }, [fetchServers, fetchGroups]);

  const handleServerClick = useCallback((server: Server) => {
    selectServer(server.id);
    onConnect?.(server);
  }, [selectServer, onConnect]);

  const handleServerContextMenu = useCallback((server: Server, event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    selectServer(server.id);
    setContextMenu({
      isOpen: true,
      position: { x: event.clientX, y: event.clientY },
      server,
    });
  }, [selectServer]);

  const handleCloseContextMenu = useCallback(() => {
    setContextMenu((prev) => ({ ...prev, isOpen: false }));
  }, []);

  const handleToggleGroup = useCallback((groupId: string) => {
    setExpandedGroups((prev) => {
      const newSet = new Set(prev);
      if (newSet.has(groupId)) {
        newSet.delete(groupId);
      } else {
        newSet.add(groupId);
      }
      return newSet;
    });
  }, []);

  const handleConnectFromMenu = useCallback(() => {
    if (contextMenu.server) {
      onConnect?.(contextMenu.server);
    }
    handleCloseContextMenu();
  }, [contextMenu.server, onConnect, handleCloseContextMenu]);

  const handleNewSessionFromMenu = useCallback(() => {
    if (contextMenu.server) {
      onNewSession?.(contextMenu.server);
    }
    handleCloseContextMenu();
  }, [contextMenu.server, onNewSession, handleCloseContextMenu]);

  const handleEditFromMenu = useCallback(() => {
    if (contextMenu.server) {
      onEditServer?.(contextMenu.server);
    }
    handleCloseContextMenu();
  }, [contextMenu.server, onEditServer, handleCloseContextMenu]);

  const handleDeleteFromMenu = useCallback(() => {
    if (contextMenu.server) {
      setDeleteConfirm(contextMenu.server);
    }
    handleCloseContextMenu();
  }, [contextMenu.server, handleCloseContextMenu]);

  const handleConfirmDelete = useCallback(async () => {
    if (!deleteConfirm) return;

    const serverName = deleteConfirm.name;
    try {
      await deleteServer(deleteConfirm.id);
      notifySuccess('Server Deleted', `${serverName} has been removed.`);
    } catch (err) {
      console.error('Failed to delete server:', err);
      notifyError('Delete Failed', `Failed to delete ${serverName}.`);
    }
    setDeleteConfirm(null);
  }, [deleteConfirm, deleteServer, notifySuccess, notifyError]);

  const handleCancelDelete = useCallback(() => {
    setDeleteConfirm(null);
  }, []);

  // Context menu items
  const contextMenuItems: ContextMenuItem[] = useMemo(() => {
    const isConnected = contextMenu.server ? connectedServerIds.has(contextMenu.server.id) : false;
    const items: ContextMenuItem[] = [
      {
        id: 'connect',
        label: isConnected ? 'Reconnect' : 'Connect',
        icon: <Plug className="w-4 h-4" />,
        onClick: handleConnectFromMenu,
      },
    ];

    // Add "New Session" option only for connected servers
    if (isConnected) {
      items.push({
        id: 'new-session',
        label: 'New Session',
        icon: <PlusCircle className="w-4 h-4" />,
        onClick: handleNewSessionFromMenu,
      });
    }

    items.push(
      {
        id: 'edit',
        label: 'Edit',
        icon: <Edit className="w-4 h-4" />,
        onClick: handleEditFromMenu,
      },
      {
        id: 'divider',
        label: '',
        onClick: () => {},
        divider: true,
      },
      {
        id: 'delete',
        label: 'Delete',
        icon: <Trash2 className="w-4 h-4" />,
        onClick: handleDeleteFromMenu,
        danger: true,
        disabled: isConnected,
      }
    );

    return items;
  }, [contextMenu.server, connectedServerIds, handleConnectFromMenu, handleNewSessionFromMenu, handleEditFromMenu, handleDeleteFromMenu]);

  return (
    <>
      <div className="h-full flex flex-col bg-gray-800/50">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-700/50">
          <div className="flex items-center gap-2">
            <ServerIcon className="w-4 h-4 text-gray-400" />
            <h2 className="text-sm font-semibold text-gray-200">Servers</h2>
          </div>
          <button
            className={cn(
              'p-1.5 rounded-md',
              'text-gray-400 hover:text-white',
              'hover:bg-gray-700/50 transition-colors duration-150',
              'focus:outline-none focus:ring-1 focus:ring-gray-500'
            )}
            onClick={onAddServer}
            aria-label="Add server"
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-2 py-2">
          {/* Loading State */}
          {loading && (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="w-5 h-5 text-gray-400 animate-spin" />
              <span className="ml-2 text-sm text-gray-400">Loading servers...</span>
            </div>
          )}

          {/* Error State */}
          {error && !loading && (
            <div className="mx-2 my-4 p-3 rounded-md bg-red-900/20 border border-red-800/30">
              <p className="text-sm text-red-400">{error}</p>
              <button
                className="mt-2 text-xs text-red-300 hover:text-red-200 underline"
                onClick={clearError}
              >
                Dismiss
              </button>
            </div>
          )}

          {/* Empty State */}
          {!loading && !error && groupedServers.length === 0 && ungroupedServers.length === 0 && (
            <div className="flex flex-col items-center justify-center py-8 px-4">
              <ServerIcon className="w-8 h-8 text-gray-600 mb-3" />
              <p className="text-sm text-gray-400 text-center mb-4">
                No servers configured yet.
                <br />
                Add your first server to get started.
              </p>
              <button
                className={cn(
                  'flex items-center gap-2 px-4 py-2 rounded-md',
                  'bg-blue-600 hover:bg-blue-500',
                  'text-sm font-medium text-white',
                  'transition-colors duration-150',
                  'focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 focus:ring-offset-gray-800'
                )}
                onClick={onAddServer}
              >
                <Plus className="w-4 h-4" />
                Add Server
              </button>
            </div>
          )}

          {/* Grouped Servers */}
          {!loading && groupedServers.map(({ group, servers }) => (
            <GroupSection
              key={group.id}
              group={group}
              servers={servers}
              isExpanded={expandedGroups.has(group.id)}
              onToggle={() => handleToggleGroup(group.id)}
              selectedServerId={selectedServerId}
              connectedServerIds={connectedServerIds}
              sessionCounts={sessionCounts}
              onServerClick={handleServerClick}
              onServerContextMenu={handleServerContextMenu}
            />
          ))}

          {/* Ungrouped Servers */}
          {!loading && ungroupedServers.length > 0 && (
            <div className="mt-2">
              {groupedServers.length > 0 && (
                <div className="px-2 py-1.5 text-xs font-medium text-gray-500 uppercase tracking-wider">
                  Ungrouped
                </div>
              )}
              <div className="space-y-0.5">
                {ungroupedServers.map((server) => (
                  <ServerItem
                    key={server.id}
                    server={server}
                    isSelected={selectedServerId === server.id}
                    isConnected={connectedServerIds.has(server.id)}
                    sessionCount={sessionCounts.get(server.id) || 0}
                    onClick={handleServerClick}
                    onContextMenu={handleServerContextMenu}
                  />
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Footer with Add Server button (when not empty) */}
        {!loading && (groupedServers.length > 0 || ungroupedServers.length > 0) && (
          <div className="px-3 py-2 border-t border-gray-700/50">
            <button
              className={cn(
                'w-full flex items-center justify-center gap-2 px-3 py-2 rounded-md',
                'text-sm text-gray-400 hover:text-white',
                'hover:bg-gray-700/50 transition-colors duration-150',
                'focus:outline-none focus:ring-1 focus:ring-gray-500'
              )}
              onClick={onAddServer}
            >
              <Plus className="w-4 h-4" />
              Add Server
            </button>
          </div>
        )}
      </div>

      {/* Context Menu */}
      <ContextMenu
        isOpen={contextMenu.isOpen}
        position={contextMenu.position}
        items={contextMenuItems}
        onClose={handleCloseContextMenu}
      />

      {/* Delete Confirmation Dialog */}
      <ConfirmDialog
        isOpen={deleteConfirm !== null}
        title="Delete Server"
        message={`Are you sure you want to delete "${deleteConfirm?.name}"? This action cannot be undone.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={handleConfirmDelete}
        onCancel={handleCancelDelete}
      />
    </>
  );
}

export type { ServerListProps };
