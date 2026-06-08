import { Monitor, MoreVertical, Wifi } from 'lucide-react';
import { cn } from '../../lib/utils';
import type { Server } from '../../stores/serverStore';

interface ServerItemProps {
  /** Server data to display */
  server: Server;
  /** Whether this server is currently selected */
  isSelected?: boolean;
  /** Whether this server has an active connection */
  isConnected?: boolean;
  /** Number of active sessions for this server */
  sessionCount?: number;
  /** Callback when the server item is clicked */
  onClick?: (server: Server) => void;
  /** Callback when the context menu button is clicked */
  onContextMenu?: (server: Server, event: React.MouseEvent) => void;
}

/**
 * Individual server item component for the server list
 * Displays server info with status indicators and context menu
 */
export function ServerItem({
  server,
  isSelected = false,
  isConnected = false,
  sessionCount = 0,
  onClick,
  onContextMenu,
}: ServerItemProps) {
  const handleClick = () => {
    onClick?.(server);
  };

  const handleContextMenuClick = (event: React.MouseEvent) => {
    event.stopPropagation();
    onContextMenu?.(server, event);
  };

  const connectionString = `${server.username}@${server.host}:${server.port}`;

  return (
    <div
      className={cn(
        'group flex items-center gap-3 px-2.5 py-2 rounded-lg cursor-pointer border',
        'transition-all duration-150 ease-out',
        'hover:bg-tokyo-bg-hl hover:border-tokyo-selection',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue',
        isSelected
          ? 'bg-tokyo-selection border-tokyo-blue text-white'
          : 'border-transparent text-tokyo-fg'
      )}
      onClick={handleClick}
      role="button"
      tabIndex={0}
      aria-pressed={isSelected}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleClick();
        }
      }}
    >
      {/* Server Icon */}
      <div
        className={cn(
          'flex-shrink-0 w-8 h-8 rounded-lg flex items-center justify-center border',
          isConnected
            ? 'bg-tokyo-bg-hl border-tokyo-green'
            : 'bg-tokyo-bg-dark border-tokyo-bg-hl'
        )}
      >
        <Monitor
          className={cn(
            'w-4 h-4',
            isConnected ? 'text-tokyo-green' : 'text-tokyo-comment'
          )}
        />
      </div>

      {/* Server Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              'text-sm font-medium truncate',
              isSelected ? 'text-white' : 'text-tokyo-fg'
            )}
          >
            {server.name}
          </span>
          {/* Connection Status Indicator */}
          {isConnected && (
            <div className="flex items-center gap-1 flex-shrink-0">
              <Wifi className="w-3 h-3 text-tokyo-green" />
              {sessionCount > 1 && (
                <span className="min-w-[1rem] h-4 inline-flex items-center justify-center rounded-full bg-tokyo-bg-hl px-1 text-[10px] font-semibold text-tokyo-green">
                  {sessionCount}
                </span>
              )}
            </div>
          )}
        </div>
        <span className="text-xs text-tokyo-comment truncate block">
          {connectionString}
        </span>
      </div>

      {/* Context Menu Button (shows on hover) */}
      <button
        className={cn(
          'flex-shrink-0 p-1 rounded-md opacity-0 group-hover:opacity-100',
          'transition-opacity duration-150 text-tokyo-comment',
          'hover:bg-tokyo-bg hover:text-tokyo-fg focus:opacity-100 focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
        )}
        onClick={handleContextMenuClick}
        aria-label={`More options for ${server.name}`}
      >
        <MoreVertical className="w-4 h-4" />
      </button>
    </div>
  );
}

export type { ServerItemProps };
