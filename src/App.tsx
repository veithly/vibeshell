import { useCallback, useState, useRef, useEffect, useMemo, memo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Plus,
  Zap,
  FolderOpen,
  Settings as SettingsIcon,
  ArrowLeft,
  Terminal as TerminalIcon,
  ArrowRightLeft,
} from 'lucide-react';
import { cn } from './lib/utils';
import { safeInvoke } from './lib/tauri';
import { useSessionStore } from './stores/sessionStore';
import { useNavigationStore } from './stores/navigationStore';
import { useNotificationStore } from './stores/notificationStore';
import { useSettingsStore, themes } from './stores/settingsStore';
import { SessionTabs } from './components/SessionTabs';
import { Settings } from './components/Settings';
import { TitleBar } from './components/TitleBar';
import { Terminal, TerminalHandle } from './components/Terminal';
import { SftpPanel, SftpPanelHandle } from './components/SftpPanel';
import { ServerStatus } from './components/ServerStatus';
import { ServerList } from './components/ServerList';
import { AddServerDialog } from './components/AddServerDialog';
import { EditServerDialog } from './components/EditServerDialog';
import { ConnectDialog } from './components/ConnectDialog';
import { SelectServerDialog } from './components/SelectServerDialog';
import { QuickCommandDialog } from './components/QuickCommandDialog';
import { ConfirmDialog } from './components/ConfirmDialog';
import { Notifications } from './components/Notifications';
import { FingerprintVerificationDialog, FingerprintManagerDialog } from './components/FingerprintDialog';
import { SnippetManagerDialog } from './components/SnippetManager/SnippetManagerDialog';
import { TunnelPanelDialog } from './components/TunnelPanel/TunnelPanelDialog';
import type { Server } from './stores/serverStore';

interface SidebarActionProps {
  icon: React.ReactNode;
  label: string;
  shortcut?: string;
  onClick?: () => void;
}

// Memoized sidebar action button for performance
const SidebarAction = memo(function SidebarAction({ icon, label, shortcut, onClick }: SidebarActionProps) {
  return (
    <button
      className={cn(
        'flex items-center gap-3 w-full px-3 py-2 rounded-md text-sm',
        'text-tokyo-fg transition-colors duration-150',
        'hover:bg-tokyo-bg-hl hover:text-white',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
      )}
      onClick={onClick}
    >
      {icon}
      <span className="flex-1 text-left">{label}</span>
      {shortcut && (
        <span className="text-xs text-tokyo-comment">{shortcut}</span>
      )}
    </button>
  );
});

function App() {
  const { t } = useTranslation();
  const { sessions, activeSessionId, setActiveSession, killSession, killLocalShellSession, removeSession, connectWithCredentials } = useSessionStore();
  const { currentView, goToMain, goToSettings } = useNavigationStore();
  const { warning: notifyWarning } = useNotificationStore();
  const { settings, initializeSettings } = useSettingsStore();

  // Dialog states
  const [isAddServerOpen, setIsAddServerOpen] = useState(false);
  const [isConnectOpen, setIsConnectOpen] = useState(false);
  const [isQuickCommandOpen, setIsQuickCommandOpen] = useState(false);
  const [isEditServerOpen, setIsEditServerOpen] = useState(false);
  const [isSelectServerOpen, setIsSelectServerOpen] = useState(false);
  const [isSnippetManagerOpen, setIsSnippetManagerOpen] = useState(false);
  const [isTunnelPanelOpen, setIsTunnelPanelOpen] = useState(false);
  const [serverToConnect, setServerToConnect] = useState<Server | null>(null);
  const [serverToEdit, setServerToEdit] = useState<Server | null>(null);

  // Close session confirmation state
  const [sessionToClose, setSessionToClose] = useState<string | null>(null);

  // Refs
  const terminalRef = useRef<TerminalHandle>(null);
  const sftpPanelRef = useRef<SftpPanelHandle>(null);

  // Initialize settings on mount
  useEffect(() => {
    initializeSettings();
  }, [initializeSettings]);

  // Apply theme CSS variables when theme changes
  useEffect(() => {
    const currentTheme = themes.find(t => t.name === settings.appearance.theme);
    if (!currentTheme) return;

    const root = document.documentElement;
    // Core theme colors (dynamic per theme)
    root.style.setProperty('--tokyo-bg', currentTheme.colors.bg);
    root.style.setProperty('--tokyo-bg-dark', currentTheme.colors.bgDark);
    root.style.setProperty('--tokyo-bg-hl', currentTheme.colors.bgHl);
    root.style.setProperty('--tokyo-fg', currentTheme.colors.fg);
    root.style.setProperty('--tokyo-fg-dark', currentTheme.colors.fgDark);
    root.style.setProperty('--tokyo-comment', currentTheme.colors.fgDark);
    root.style.setProperty('--tokyo-selection', currentTheme.colors.bgHl);
    root.style.setProperty('--tokyo-blue', currentTheme.colors.accent);

    console.log('[App] Applied theme:', currentTheme.name);
  }, [settings.appearance.theme]);

  // Note: Debug listener for session-output removed for performance.
  // The Terminal component handles its own session output listening.

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = async (event: KeyboardEvent) => {
      // F12: Open DevTools
      if (event.key === 'F12') {
        event.preventDefault();
        await safeInvoke('open_devtools');
        return;
      }

      // Ignore if user is typing in an input field
      const target = event.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
        return;
      }

      const isCtrl = event.ctrlKey || event.metaKey;

      if (isCtrl) {
        switch (event.key.toLowerCase()) {
          case 'n':
            // Ctrl+N: New connection (open Add Server)
            event.preventDefault();
            setIsAddServerOpen(true);
            break;
          case 'k':
            // Ctrl+K: Quick command
            event.preventDefault();
            setIsQuickCommandOpen(true);
            break;
          case ',':
            // Ctrl+,: Settings
            event.preventDefault();
            goToSettings();
            break;
          case 'w':
            // Ctrl+W: Close active session
            event.preventDefault();
            if (activeSessionId) {
              const activeSession = sessions.find((s) => s.id === activeSessionId);
              if (activeSession?.state === 'connected' || activeSession?.state === 'connecting') {
                setSessionToClose(activeSessionId);
              } else {
                removeSession(activeSessionId);
              }
            }
            break;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeSessionId, sessions, goToSettings, removeSession]);

  // Memoize connectedServerIds to prevent unnecessary re-renders of child components
  const connectedServerIds = useMemo(() => new Set(
    sessions
      .filter((s) => s.state === 'connected' || s.state === 'connecting')
      .map((s) => s.serverId)
  ), [sessions]);

  // Memoize activeSession lookup
  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId),
    [sessions, activeSessionId]
  );

  const handleConnected = useCallback((sessionId: string) => {
    console.log('[App] handleConnected called with sessionId:', sessionId);
    setActiveSession(sessionId);

    // Note: We do NOT call attachSession here because session_connect already
    // sets up output forwarding in the backend. Calling attachSession would create
    // a duplicate forwarder, causing double character output (each keystroke echoed twice).

    // Focus the terminal once connected
    setTimeout(() => {
      console.log('[App] Focusing terminal for session:', sessionId);
      terminalRef.current?.focus();
    }, 100);
  }, [setActiveSession]);

  const handleConnect = useCallback(async (server: Server) => {
    console.log('[App] handleConnect called for server:', server.name);

    // Try to get saved credentials first
    const credResult = await safeInvoke<{
      id: string;
      server_name: string;
      auth_type: string;
      credential: string;
      passphrase: string | null;
      key_path: string | null;
      created_at: number;
    } | null>('get_credential', { request: { serverName: server.name } });

    if (credResult.success && credResult.data) {
      // Saved credentials found - auto-connect
      console.log('[App] Found saved credentials, auto-connecting...');
      const cred = credResult.data;
      const authType = (cred.auth_type === 'key' || cred.auth_type === 'key_with_passphrase') ? 'key' : 'password';

      const session = await connectWithCredentials(
        server.name,
        authType,
        cred.credential,
        cred.passphrase || undefined,
        80,
        24
      );

      if (session) {
        console.log('[App] Auto-connect successful, session:', session.id);
        handleConnected(session.id);
      } else {
        console.log('[App] Auto-connect failed, showing dialog');
        // If auto-connect fails, show the dialog for manual entry
        setServerToConnect(server);
        setIsConnectOpen(true);
      }
    } else {
      // No saved credentials - show the dialog
      console.log('[App] No saved credentials, opening connection dialog');
      setServerToConnect(server);
      setIsConnectOpen(true);
    }
  }, [connectWithCredentials, handleConnected]);

  const handleAddServer = useCallback(() => {
    setIsAddServerOpen(true);
  }, []);

  const handleEditServer = useCallback((server: Server) => {
    setServerToEdit(server);
    setIsEditServerOpen(true);
  }, []);

  const handleNewSession = useCallback(() => {
    // Open the server selection dialog instead of creating a dummy session
    setIsSelectServerOpen(true);
  }, []);

  // Handle opening a new session for an already connected server
  const handleNewSessionForServer = useCallback((server: Server) => {
    // This creates a new session for an already connected server
    // Use the same connect flow which will auto-connect if credentials are saved
    handleConnect(server);
  }, [handleConnect]);

  const handleQuickCommand = useCallback(() => {
    setIsQuickCommandOpen(true);
  }, []);

  const handleOpenSnippets = useCallback(() => {
    setIsSnippetManagerOpen(true);
  }, []);

  const handleOpenTunnels = useCallback(() => {
    if (!activeSession) {
      notifyWarning('No Active Session', 'Connect to a server first to manage tunnels.');
      return;
    }
    setIsTunnelPanelOpen(true);
  }, [activeSession, notifyWarning]);

  const handleOpenSftp = useCallback(() => {
    if (!activeSession) {
      notifyWarning('No Active Session', 'Connect to a server first to use SFTP.');
      return;
    }
    // Expand SFTP panel for the active session
    sftpPanelRef.current?.expand();
  }, [activeSession, notifyWarning]);

  // Terminal data handler - the Terminal component handles sending data to backend
  // This callback is kept minimal for performance
  const handleData = useCallback((_data: string) => {
    // No-op: Terminal component handles input directly
  }, []);

  const handleExpandSftp = useCallback(() => {
    console.log('Expand SFTP panel to full view clicked');
    // TODO: Open full SFTP file manager view
    notifyWarning('Coming Soon', 'Full SFTP file manager view is under development.');
  }, [notifyWarning]);

  // Handle close session confirmation
  const handleConfirmCloseSession = useCallback(async () => {
    if (!sessionToClose) return;

    const sessionId = sessionToClose;
    const session = sessions.find((s) => s.id === sessionId);
    setSessionToClose(null);

    // Use appropriate kill function based on session type
    const success = session?.sessionType === 'local'
      ? await killLocalShellSession(sessionId)
      : await killSession(sessionId);

    if (!success) {
      removeSession(sessionId);
    }
  }, [sessionToClose, sessions, killSession, killLocalShellSession, removeSession]);

  const handleCancelCloseSession = useCallback(() => {
    setSessionToClose(null);
  }, []);

  const isSettingsView = currentView === 'settings';
  const sessionToCloseObj = sessionToClose ? sessions.find((s) => s.id === sessionToClose) : null;

  return (
    <div className="h-screen flex flex-col bg-tokyo-bg">
      {/* Settings View - always mounted, toggled via CSS to preserve Terminal state */}
      <div
        className="h-screen flex flex-col bg-tokyo-bg absolute inset-0 z-10"
        style={{ display: isSettingsView ? 'flex' : 'none' }}
      >
        <TitleBar />
        <Notifications />
        <header className="h-10 flex items-center px-4 bg-tokyo-bg-dark border-b border-tokyo-bg-hl">
          <button
            className={cn(
              'flex items-center gap-2 px-3 py-1.5 rounded-md',
              'text-tokyo-fg hover:text-white hover:bg-tokyo-bg-hl',
              'transition-colors duration-150'
            )}
            onClick={goToMain}
          >
            <ArrowLeft className="w-4 h-4" />
            <span className="text-sm">{t('common.back')}</span>
          </button>
          <h1 className="ml-4 text-white font-semibold">{t('settings.title')}</h1>
        </header>
        <div className="flex-1 overflow-y-auto bg-tokyo-bg">
          <Settings />
        </div>
      </div>

      {/* Main View - always mounted, hidden when settings is active */}
      <div
        className="h-full flex flex-col flex-1"
        style={{ visibility: isSettingsView ? 'hidden' : 'visible' }}
      >
        <TitleBar
          activeSessionName={activeSession?.serverName}
          activeSessionType={activeSession?.sessionType}
          activeSessionState={activeSession?.state}
        />
        <Notifications />

        <div className="flex-1 flex overflow-hidden">
          <aside className="w-56 flex flex-col border-r border-tokyo-bg-hl bg-tokyo-bg-dark flex-shrink-0">
            <div className="flex-1 overflow-hidden">
              <ServerList
                onConnect={handleConnect}
                onAddServer={handleAddServer}
                onEditServer={handleEditServer}
                connectedServerIds={connectedServerIds}
                onNewSession={handleNewSessionForServer}
              />
            </div>

            <div className="border-t border-tokyo-bg-hl p-2 space-y-1">
              <div className="px-2 py-1.5 text-xs font-medium text-tokyo-comment uppercase tracking-wider">
                {t('common.actions')}
              </div>
              <SidebarAction
                icon={<Plus className="w-4 h-4" />}
                label={t('sidebar.addServer')}
                shortcut="Ctrl+N"
                onClick={handleAddServer}
              />
              <SidebarAction
                icon={<Zap className="w-4 h-4" />}
                label={t('sidebar.quickCmd')}
                shortcut="Ctrl+K"
                onClick={handleQuickCommand}
              />
              <SidebarAction
                icon={<FolderOpen className="w-4 h-4" />}
                label={t('sidebar.sftp')}
                onClick={handleOpenSftp}
              />
              <SidebarAction
                icon={<ArrowRightLeft className="w-4 h-4" />}
                label={t('sidebar.tunnels')}
                onClick={handleOpenTunnels}
              />
              <SidebarAction
                icon={<TerminalIcon className="w-4 h-4" />}
                label={t('sidebar.snippets')}
                onClick={handleOpenSnippets}
              />
              <SidebarAction
                icon={<SettingsIcon className="w-4 h-4" />}
                label={t('sidebar.settings')}
                shortcut="Ctrl+,"
                onClick={goToSettings}
              />
            </div>
          </aside>

          <main className="flex-1 flex flex-col min-w-0 relative">
            <SessionTabs onNewSession={handleNewSession} />

            <div className="flex-1 min-h-0 flex flex-col">
              {activeSession ? (
                <>
                  <div className="flex-1 min-h-0">
                    <Terminal ref={terminalRef} sessionId={activeSession.id} onData={handleData} />
                  </div>
                  <ServerStatus
                    sessionId={activeSession.id}
                    defaultCollapsed={!settings.serverStatus.defaultExpanded}
                    defaultRefreshInterval={settings.serverStatus.refreshInterval}
                  />
                </>
              ) : (
                <div className="h-full flex items-center justify-center bg-tokyo-bg">
                  <div className="text-center">
                    <div className="text-6xl mb-4 text-tokyo-comment">{'>'}_</div>
                    <p className="text-tokyo-fg text-lg mb-2">{t('session.noActiveSession')}</p>
                    <p className="text-tokyo-comment text-sm mb-6">
                      {t('session.noActiveSessionDesc')}
                    </p>
                    <button
                      className={cn(
                        'inline-flex items-center gap-2 px-4 py-2 rounded-md',
                        'bg-tokyo-blue hover:bg-tokyo-blue/80 text-white',
                        'transition-colors duration-150'
                      )}
                      onClick={handleNewSession}
                    >
                      <Plus className="w-4 h-4" />
                      {t('session.newSession')}
                    </button>
                  </div>
                </div>
              )}
            </div>

            <SftpPanel
              ref={sftpPanelRef}
              sessionId={activeSession?.id}
              sessionType={activeSession?.sessionType}
              defaultCollapsed={true}
              onExpand={handleExpandSftp}
            />
          </main>
        </div>
      </div>

      {/* Add Server Dialog */}
      <AddServerDialog
        isOpen={isAddServerOpen}
        onClose={() => setIsAddServerOpen(false)}
      />

      {/* Edit Server Dialog */}
      <EditServerDialog
        isOpen={isEditServerOpen}
        server={serverToEdit}
        onClose={() => {
          setIsEditServerOpen(false);
          setServerToEdit(null);
        }}
      />

      {/* Connect Dialog */}
      <ConnectDialog
        isOpen={isConnectOpen}
        server={serverToConnect}
        onClose={() => {
          setIsConnectOpen(false);
          setServerToConnect(null);
        }}
        onConnected={handleConnected}
      />

      {/* Quick Command Dialog */}
      <QuickCommandDialog
        isOpen={isQuickCommandOpen}
        onClose={() => setIsQuickCommandOpen(false)}
      />

      {/* Select Server Dialog (for new session) */}
      <SelectServerDialog
        isOpen={isSelectServerOpen}
        onClose={() => setIsSelectServerOpen(false)}
        onSelectServer={handleConnect}
        onAddServer={handleAddServer}
        connectedServerIds={connectedServerIds}
      />

      {/* Close Session Confirmation Dialog */}
      <ConfirmDialog
        isOpen={sessionToClose !== null}
        title={sessionToCloseObj?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
        message={
          sessionToCloseObj?.sessionType === 'local'
            ? t('session.closeLocalShellConfirm', { name: sessionToCloseObj?.serverName })
            : t('session.closeSessionConfirm', { name: sessionToCloseObj?.serverName })
        }
        confirmLabel={sessionToCloseObj?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
        cancelLabel={t('common.cancel')}
        variant="danger"
        onConfirm={handleConfirmCloseSession}
        onCancel={handleCancelCloseSession}
      />

      {/* Fingerprint Verification Dialog */}
      <FingerprintVerificationDialog />

      {/* Fingerprint Manager Dialog */}
      <FingerprintManagerDialog />

      {/* Snippet Manager Dialog */}
      <SnippetManagerDialog
        isOpen={isSnippetManagerOpen}
        onClose={() => setIsSnippetManagerOpen(false)}
      />

      {/* Tunnel Panel Dialog */}
      <TunnelPanelDialog
        isOpen={isTunnelPanelOpen}
        serverId={activeSession?.serverId || ''}
        sessionId={activeSession?.id}
        onClose={() => setIsTunnelPanelOpen(false)}
      />
    </div>
  );
}

export default App;
