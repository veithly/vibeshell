import { useState, useCallback, memo, useRef, useEffect, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, X, Loader2, AlertCircle, Wifi, Monitor, Circle, RefreshCw, Code2 } from 'lucide-react';
import { cn } from '../../lib/utils';
import { useSessionStore, type Session, type SessionState, type SessionType } from '../../stores/sessionStore';
import { useRecordingStore } from '../../stores/recordingStore';
import { useFileWorkspaceStore, type FileWorkspaceTab as FileTabModel } from '../../stores/fileWorkspaceStore';
import { ConfirmDialog } from '../ConfirmDialog';
import { FileIcon } from '../SftpPanel/FileIcon';

interface SessionTabsProps {
  /** Callback when "New Session" is clicked */
  onNewSession?: () => void;
  /** Callback when reconnecting a disconnected SSH session is requested */
  onReconnectSession?: (session: Session) => void;
  /** Commands pinned to the right of the scrollable session strip. */
  rightActions?: ReactNode;
}

/**
 * Get status indicator for a session state and type
 */
function getStateIndicator(
  state: SessionState,
  sessionType: SessionType = 'ssh',
  purpose?: Session['purpose']
) {
  switch (state) {
    case 'connecting':
      return <Loader2 className="w-3 h-3 animate-spin text-tokyo-yellow" />;
    case 'connected':
      // Show different icon for local vs SSH
      if (sessionType === 'local') {
        if (purpose === 'coding_agent') {
          return <Code2 className="w-3 h-3 text-tokyo-cyan" />;
        }
        return <Monitor className="w-3 h-3 text-tokyo-cyan" />;
      }
      return <Wifi className="w-3 h-3 text-tokyo-cyan" />;
    case 'error':
      return <AlertCircle className="w-3 h-3 text-tokyo-red" />;
    case 'disconnected':
    default:
      if (sessionType === 'local') {
        if (purpose === 'coding_agent') {
          return <Code2 className="w-3 h-3 text-tokyo-comment" />;
        }
        return <span className="w-2 h-2 rounded-full bg-tokyo-cyan" />;
      }
      return <span className="w-2 h-2 rounded-full bg-tokyo-comment" />;
  }
}

interface SessionTabProps {
  session: Session;
  isActive: boolean;
  isRecording: boolean;
  onSelect: () => void;
  onReconnect?: () => void;
  onClose: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}

/**
 * Individual session tab component - memoized to prevent unnecessary re-renders
 */
const SessionTab = memo(function SessionTab({
  session,
  isActive,
  isRecording,
  onSelect,
  onReconnect,
  onClose,
  onContextMenu,
}: SessionTabProps) {
  const canReconnect =
    session.sessionType === 'ssh' && (session.state === 'disconnected' || session.state === 'error');

  const handleReconnect = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onReconnect?.();
  }, [onReconnect]);

  const handleClose = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    onClose();
  }, [onClose]);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onSelect();
    }
  }, [onSelect]);

  return (
    <div
      className={cn(
        'session-tab-item group flex h-8 items-center gap-2 px-2.5 rounded-lg cursor-pointer border',
        'transition-all duration-150 ease-out',
        'min-w-[120px] max-w-[220px]',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue',
        isActive
          ? 'bg-tokyo-bg border-tokyo-cyan text-tokyo-fg'
          : 'bg-transparent border-transparent text-tokyo-comment hover:text-tokyo-fg hover:bg-tokyo-bg-hl hover:border-tokyo-selection'
      )}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      role="tab"
      aria-selected={isActive}
      tabIndex={0}
      onKeyDown={handleKeyDown}
    >
      {/* Status Indicator */}
      <span className="flex-shrink-0">
        {getStateIndicator(session.state, session.sessionType, session.purpose)}
      </span>

      {/* Server Name */}
      <span className="truncate text-sm font-medium flex-1">
        {session.serverName}
      </span>

      {/* Recording Indicator */}
      {isRecording && (
        <span className="flex-shrink-0" title="Recording session">
          <Circle className="w-2.5 h-2.5 text-tokyo-red fill-tokyo-red animate-pulse" />
        </span>
      )}

      {/* Reconnect Button */}
      {canReconnect && (
        <button
          className={cn(
            'session-tab-action flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md opacity-0 group-hover:opacity-100',
            'transition-opacity duration-150',
            'hover:bg-tokyo-bg-hl focus:opacity-100 focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
          )}
          onClick={handleReconnect}
          aria-label={`Reconnect ${session.serverName} session`}
          title={`Reconnect ${session.serverName}`}
        >
          <RefreshCw className="w-3.5 h-3.5" />
        </button>
      )}

      {/* Close Button */}
      <button
        className={cn(
          'session-tab-action flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md opacity-0 group-hover:opacity-100',
          'transition-opacity duration-150',
          'hover:bg-tokyo-bg-hl focus:opacity-100 focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
        )}
        onClick={handleClose}
        aria-label={`Close ${session.serverName} session`}
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
});

interface FileTabProps {
  tab: FileTabModel;
  isActive: boolean;
  onSelect: () => void;
  onClose: () => void;
}

const FileTab = memo(function FileTab({ tab, isActive, onSelect, onClose }: FileTabProps) {
  return (
    <div
      className={cn(
        'session-tab-item group flex h-8 min-w-[120px] max-w-[240px] cursor-pointer items-center gap-2 rounded-lg border px-2.5',
        'transition-all duration-150 ease-out focus:outline-none focus:ring-1 focus:ring-tokyo-blue',
        isActive
          ? 'bg-tokyo-bg border-tokyo-cyan text-tokyo-fg'
          : 'bg-transparent border-transparent text-tokyo-comment hover:border-tokyo-selection hover:bg-tokyo-bg-hl hover:text-tokyo-fg'
      )}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      role="tab"
      aria-selected={isActive}
      tabIndex={0}
      title={tab.path}
    >
      <FileIcon filename={tab.name} isDirectory={false} />
      <span className="min-w-0 flex-1 truncate text-sm font-medium">{tab.name}</span>
      {tab.dirty && <span className="h-2 w-2 flex-shrink-0 rounded-full bg-tokyo-orange" />}
      <button
        className={cn(
          'session-tab-action flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md',
          'opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus:opacity-100',
          'hover:bg-tokyo-bg-hl focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
        )}
        onClick={(event) => {
          event.stopPropagation();
          onClose();
        }}
        aria-label={`Close ${tab.name}`}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
});

/**
 * Context menu for session tabs
 */
interface ContextMenuProps {
  x: number;
  y: number;
  session: Session;
  isRecording: boolean;
  onStartRecording: () => void;
  onStopRecording: () => void;
  onReconnect: () => void;
  onClose: () => void;
}

function TabContextMenu({
  x,
  y,
  session,
  isRecording,
  onStartRecording,
  onStopRecording,
  onReconnect,
  onClose,
}: ContextMenuProps) {
  const { t } = useTranslation();
  const menuRef = useRef<HTMLDivElement>(null);
  const canReconnect =
    session.sessionType === 'ssh' && (session.state === 'disconnected' || session.state === 'error');

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [onClose]);

  // Clamp menu position to viewport
  const clampedX = Math.min(x, window.innerWidth - 200);
  const clampedY = Math.min(y, window.innerHeight - 120);

  return (
    <div
      ref={menuRef}
      role="menu"
      className="fixed z-50 bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg py-1 min-w-[180px]"
      style={{ left: clampedX, top: clampedY }}
    >
      <div className="px-3 py-1.5 text-xs text-tokyo-comment border-b border-tokyo-bg-hl mb-1 font-medium">
        {session.serverName}
      </div>
      {canReconnect && (
        <button
          role="menuitem"
          className="w-full text-left px-3 py-2 text-sm text-tokyo-fg hover:bg-tokyo-bg-hl transition-colors
                     flex items-center gap-2.5 cursor-pointer"
          onClick={() => { onReconnect(); onClose(); }}
        >
          <RefreshCw className="w-3 h-3 text-tokyo-cyan" />
          {t('session.reconnect')}
        </button>
      )}
      {session.state === 'connected' && (
        isRecording ? (
          <button
            role="menuitem"
            className="w-full text-left px-3 py-2 text-sm text-tokyo-red hover:bg-tokyo-bg-hl transition-colors
                       flex items-center gap-2.5 cursor-pointer"
            onClick={() => { onStopRecording(); onClose(); }}
          >
            <Circle className="w-3 h-3 fill-tokyo-red" />
            {t('session.stopRecording')}
          </button>
        ) : (
          <button
            role="menuitem"
            className="w-full text-left px-3 py-2 text-sm text-tokyo-fg hover:bg-tokyo-bg-hl transition-colors
                       flex items-center gap-2.5 cursor-pointer"
            onClick={() => { onStartRecording(); onClose(); }}
          >
            <Circle className="w-3 h-3 text-tokyo-red" />
            {t('session.startRecording')}
          </button>
        )
      )}
    </div>
  );
}

/**
 * Session tabs component displaying all active sessions
 */
export function SessionTabs({ onNewSession, onReconnectSession, rightActions }: SessionTabsProps) {
  const { t } = useTranslation();
  const { sessions, activeSessionId, setActiveSession, killSession, killLocalShellSession, removeSession } =
    useSessionStore();
  const {
    tabs: fileTabs,
    activeTabId: activeFileTabId,
    activateTab: activateFileTab,
    closeTab: closeFileTab,
  } = useFileWorkspaceStore();
  const { isRecording, startRecording, stopRecording } = useRecordingStore();

  // Confirmation dialog state
  const [confirmClose, setConfirmClose] = useState<Session | null>(null);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; session: Session } | null>(null);

  const handleSelectSession = useCallback((sessionId: string) => {
    activateFileTab(null);
    setActiveSession(sessionId);
  }, [activateFileTab, setActiveSession]);

  const handleSelectFile = useCallback((tab: FileTabModel) => {
    if (sessions.some((session) => session.id === tab.sessionId)) {
      setActiveSession(tab.sessionId);
    }
    activateFileTab(tab.id);
  }, [activateFileTab, sessions, setActiveSession]);

  const handleCloseFile = useCallback((tab: FileTabModel) => {
    if (tab.dirty && !window.confirm(t('fileWorkspace.discardChanges'))) return;
    closeFileTab(tab.id);
  }, [closeFileTab, t]);

  const closeInactiveSession = useCallback(async (session: Session) => {
    const success = session.sessionType === 'local'
      ? await killLocalShellSession(session.id)
      : await killSession(session.id);

    if (!success) {
      removeSession(session.id);
    }
  }, [killSession, killLocalShellSession, removeSession]);

  const handleRequestClose = useCallback((session: Session) => {
    const hasUnsavedFiles = fileTabs.some((tab) => tab.sessionId === session.id && tab.dirty);
    if (session.state === 'connected' || session.state === 'connecting' || hasUnsavedFiles) {
      setConfirmClose(session);
    } else {
      void closeInactiveSession(session);
    }
  }, [closeInactiveSession, fileTabs]);

  const handleConfirmClose = useCallback(async () => {
    if (!confirmClose) return;

    const sessionId = confirmClose.id;
    const sessionType = confirmClose.sessionType;
    setConfirmClose(null);

    const success = sessionType === 'local'
      ? await killLocalShellSession(sessionId)
      : await killSession(sessionId);

    if (!success) {
      removeSession(sessionId);
    }
  }, [confirmClose, killSession, killLocalShellSession, removeSession]);

  const handleCancelClose = useCallback(() => {
    setConfirmClose(null);
  }, []);

  const handleContextMenu = useCallback((e: React.MouseEvent, session: Session) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, session });
  }, []);

  const handleStartRecording = useCallback(async (session: Session) => {
    await startRecording(session.id, session.serverId);
  }, [startRecording]);

  const handleStopRecording = useCallback(async (session: Session) => {
    await stopRecording(session.id);
  }, [stopRecording]);

  const handleReconnect = useCallback((session: Session) => {
    const dirtyFileCount = fileTabs.filter((tab) => tab.sessionId === session.id && tab.dirty).length;
    if (dirtyFileCount > 0 && !window.confirm(t('fileWorkspace.sessionUnsavedWarning', { count: dirtyFileCount }))) {
      return;
    }
    onReconnectSession?.(session);
  }, [fileTabs, onReconnectSession, t]);

  const confirmCloseDirtyFileCount = confirmClose
    ? fileTabs.filter((tab) => tab.sessionId === confirmClose.id && tab.dirty).length
    : 0;
  const confirmCloseMessage = confirmClose?.purpose === 'coding_agent'
    ? t('codingAgent.closeConfirm', { name: confirmClose?.serverName })
    : confirmClose?.sessionType === 'local'
      ? t('session.closeLocalShellConfirm', { name: confirmClose?.serverName })
      : t('session.closeSessionConfirm', { name: confirmClose?.serverName });

  return (
    <>
      <div className="session-tabbar flex h-11 items-center gap-1.5 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto">
          <div className="session-rail" aria-hidden="true">
            <span className="session-rail-bars">
              <i />
              <i />
              <i />
            </span>
            <span className="session-rail-count">{sessions.length + fileTabs.length}</span>
          </div>

          {sessions.map((session) => (
            <SessionTab
              key={session.id}
              session={session}
              isActive={activeFileTabId === null && activeSessionId === session.id}
              isRecording={isRecording(session.id)}
              onSelect={() => handleSelectSession(session.id)}
              onReconnect={() => handleReconnect(session)}
              onClose={() => handleRequestClose(session)}
              onContextMenu={(e) => handleContextMenu(e, session)}
            />
          ))}

          {fileTabs.map((tab) => (
            <FileTab
              key={tab.id}
              tab={tab}
              isActive={activeFileTabId === tab.id}
              onSelect={() => handleSelectFile(tab)}
              onClose={() => handleCloseFile(tab)}
            />
          ))}

          <button
            className={cn(
              'session-new-action flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-transparent',
              'bg-transparent text-tokyo-comment hover:text-tokyo-fg hover:bg-tokyo-bg',
              'hover:border-tokyo-selection transition-colors duration-150',
              'focus:outline-none focus:ring-1 focus:ring-tokyo-blue focus:ring-inset'
            )}
            onClick={onNewSession}
            aria-label={t('session.newSession')}
            title={t('session.newSession')}
          >
            <Plus className="w-4 h-4" />
          </button>
        </div>
        {rightActions}
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <TabContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          session={contextMenu.session}
          isRecording={isRecording(contextMenu.session.id)}
          onStartRecording={() => handleStartRecording(contextMenu.session)}
          onStopRecording={() => handleStopRecording(contextMenu.session)}
          onReconnect={() => handleReconnect(contextMenu.session)}
          onClose={() => setContextMenu(null)}
        />
      )}

      {/* Confirmation Dialog */}
      <ConfirmDialog
        isOpen={confirmClose !== null}
        title={confirmClose?.purpose === 'coding_agent'
          ? t('codingAgent.close')
          : confirmClose?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
        message={confirmCloseDirtyFileCount > 0
          ? `${confirmCloseMessage}\n\n${t('fileWorkspace.sessionUnsavedWarning', { count: confirmCloseDirtyFileCount })}`
          : confirmCloseMessage}
        confirmLabel={confirmClose?.purpose === 'coding_agent'
          ? t('codingAgent.close')
          : confirmClose?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
        cancelLabel={t('common.cancel')}
        variant="danger"
        onConfirm={handleConfirmClose}
        onCancel={handleCancelClose}
      />
    </>
  );
}

export type { SessionTabsProps };
