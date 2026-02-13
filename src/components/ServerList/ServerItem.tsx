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
        'group flex items-center gap-3 px-3 py-2 rounded-md cursor-pointer',
        'transition-colors duration-150',
        'hover:bg-gray-700/50',
        isSelected && 'bg-gray-700/70 border-l-2 border-blue-500',
        !isSelected && 'border-l-2 border-transparent'
      )}
      onClick={handleClick}
      role="button"
      tabIndex={0}
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
          'flex-shrink-0 w-8 h-8 rounded-md flex items-center justify-center',
          'bg-gray-700/50',
          isConnected && 'bg-green-900/30'
        )}
      >
        <Monitor
          className={cn(
            'w-4 h-4',
            isConnected ? 'text-green-400' : 'text-gray-400'
          )}
        />
      </div>

      {/* Server Info */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span
            className={cn(
              'text-sm font-medium truncate',
              isSelected ? 'text-white' : 'text-gray-200'
            )}
          >
            {server.name}
          </span>
          {/* Connection Status Indicator */}
          {isConnected && (
            <div className="flex items-center gap-1 flex-shrink-0">
              <Wifi className="w-3 h-3 text-green-400" />
              {sessionCount > 1 && (
                <span className="text-[10px] bg-green-700/50 text-green-300 px-1 rounded-full font-medium">
                  {sessionCount}
                </span>
              )}
            </div>
          )}
        </div>
        <span className="text-xs text-gray-500 truncate block">
          {connectionString}
        </span>
      </div>

      {/* Context Menu Button (shows on hover) */}
      <button
        className={cn(
          'flex-shrink-0 p-1 rounded opacity-0 group-hover:opacity-100',
          'transition-opacity duration-150',
          'hover:bg-gray-600/50 focus:opacity-100 focus:outline-none focus:ring-1 focus:ring-gray-500'
        )}
        onClick={handleContextMenuClick}
        aria-label={`More options for ${server.name}`}
      >
        <MoreVertical className="w-4 h-4 text-gray-400" />
      </button>
    </div>
  );
}

export type { ServerItemProps };
