import { useCallback, useState, useRef, useEffect, useMemo, memo, lazy, Suspense } from 'react';
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
import { useSessionStore, type Session } from './stores/sessionStore';
import { useNavigationStore } from './stores/navigationStore';
import { useNotificationStore } from './stores/notificationStore';
import { useSettingsStore, themes } from './stores/settingsStore';
import { SessionTabs } from './components/SessionTabs';
import { TitleBar } from './components/TitleBar';
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
import { useServerStore, type Server } from './stores/serverStore';
import type { TerminalHandle } from './components/Terminal';

const Settings = lazy(() => import('./components/Settings').then((mod) => ({ default: mod.Settings })));
const Terminal = lazy(() => import('./components/Terminal').then((mod) => ({ default: mod.Terminal })));

interface SidebarActionProps {
  icon: React.ReactNode;
  label: string;
  shortcut?: string;
  variant?: 'default' | 'primary';
  onClick?: () => void;
}

const SidebarAction = memo(function SidebarAction({ icon, label, shortcut, variant = 'default', onClick }: SidebarActionProps) {
  return (
    <button
      className={cn(
        'group flex items-center gap-2.5 w-full min-h-9 px-2.5 py-2 rounded-lg border text-sm',
        'transition-all duration-150 ease-out',
        'focus:outline-none focus:ring-1 focus:ring-tokyo-blue',
        variant === 'primary'
          ? 'border-tokyo-blue bg-tokyo-selection text-tokyo-fg hover:bg-tokyo-bg-hl hover:text-white'
          : 'border-transparent text-tokyo-fg hover:border-tokyo-bg-hl hover:bg-tokyo-bg-hl hover:text-white'
      )}
      onClick={onClick}
    >
      <span
        className={cn(
          'flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md',
          variant === 'primary'
            ? 'bg-tokyo-bg-hl text-tokyo-blue'
            : 'bg-tokyo-bg text-tokyo-comment group-hover:text-tokyo-fg'
        )}
        aria-hidden="true"
      >
        {icon}
      </span>
      <span className="flex-1 truncate text-left">{label}</span>
      {shortcut && (
        <span className="kbd-chip">{shortcut}</span>
      )}
    </button>
  );
});

function App() {
  const { t } = useTranslation();
  const {
    sessions,
    activeSessionId,
    setActiveSession,
    killSession,
    killLocalShellSession,
    removeSession,
    connectWithCredentials,
    fetchSessions,
    syncRemoteSessions,
  } = useSessionStore();
  const { currentView, goToMain, goToSettings } = useNavigationStore();
  const { warning: notifyWarning } = useNotificationStore();
  const { settings, initializeSettings } = useSettingsStore();
  const servers = useServerStore((state) => state.servers);
  const fetchServers = useServerStore((state) => state.fetchServers);

  const [isAddServerOpen, setIsAddServerOpen] = useState(false);
  const [isConnectOpen, setIsConnectOpen] = useState(false);
  const [isQuickCommandOpen, setIsQuickCommandOpen] = useState(false);
  const [isEditServerOpen, setIsEditServerOpen] = useState(false);
  const [isSelectServerOpen, setIsSelectServerOpen] = useState(false);
  const [isSnippetManagerOpen, setIsSnippetManagerOpen] = useState(false);
  const [isTunnelPanelOpen, setIsTunnelPanelOpen] = useState(false);
  const [serverToConnect, setServerToConnect] = useState<Server | null>(null);
  const [connectForceNew, setConnectForceNew] = useState(false);
  const [serverToEdit, setServerToEdit] = useState<Server | null>(null);
  const [sessionToClose, setSessionToClose] = useState<string | null>(null);

  const terminalRef = useRef<TerminalHandle>(null);
  const sftpPanelRef = useRef<SftpPanelHandle>(null);

  useEffect(() => {
    initializeSettings();
  }, [initializeSettings]);

  useEffect(() => {
    fetchSessions();

    const intervalId = window.setInterval(() => {
      syncRemoteSessions();
    }, 2000);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [fetchSessions, syncRemoteSessions]);

  useEffect(() => {
    const currentTheme = themes.find(t => t.name === settings.appearance.theme);
    if (!currentTheme) return;

    const root = document.documentElement;
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

  const closeInactiveSession = useCallback(async (session: Session) => {
    const success = session.sessionType === 'local'
      ? await killLocalShellSession(session.id)
      : await killSession(session.id);

    if (!success) {
      removeSession(session.id);
    }
  }, [killSession, killLocalShellSession, removeSession]);

  useEffect(() => {
    const handleKeyDown = async (event: KeyboardEvent) => {
      if (event.key === 'F12') {
        event.preventDefault();
        await safeInvoke('open_devtools');
        return;
      }

      const target = event.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
        return;
      }

      const isCtrl = event.ctrlKey || event.metaKey;

      if (isCtrl) {
        switch (event.key.toLowerCase()) {
          case 'n':
            event.preventDefault();
            setIsAddServerOpen(true);
            break;
          case 'k':
            event.preventDefault();
            setIsQuickCommandOpen(true);
            break;
          case ',':
            event.preventDefault();
            goToSettings();
            break;
          case 'w':
            event.preventDefault();
            if (activeSessionId) {
              const activeSession = sessions.find((s) => s.id === activeSessionId);
              if (activeSession?.state === 'connected' || activeSession?.state === 'connecting') {
                setSessionToClose(activeSessionId);
              } else if (activeSession) {
                void closeInactiveSession(activeSession);
              }
            }
            break;
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeSessionId, sessions, goToSettings, closeInactiveSession]);

  const connectedServerIds = useMemo(() => new Set(
    sessions
      .filter((s) => s.state === 'connected' || s.state === 'connecting')
      .map((s) => s.serverId)
  ), [sessions]);

  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId),
    [sessions, activeSessionId]
  );

  const handleConnected = useCallback((sessionId: string) => {
    console.log('[App] handleConnected called with sessionId:', sessionId);
    setActiveSession(sessionId);

    // The Terminal component attaches after its event listener is ready so
    // the initial prompt/MOTD can be replayed without losing early output.
    setTimeout(() => {
      console.log('[App] Focusing terminal for session:', sessionId);
      terminalRef.current?.focus();
    }, 100);
  }, [setActiveSession]);

  const handleConnect = useCallback(async (server: Server, options?: { forceNew?: boolean }) => {
    console.log('[App] handleConnect called for server:', server.name);
    const forceNew = options?.forceNew ?? false;

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
      console.log('[App] Found saved credentials, auto-connecting...');
      const cred = credResult.data;
      const authType = (cred.auth_type === 'key' || cred.auth_type === 'key_with_passphrase') ? 'key' : 'password';

      const session = await connectWithCredentials(
        server.name,
        authType,
        cred.credential,
        cred.passphrase || undefined,
        80,
        24,
        forceNew
      );

      if (session) {
        console.log('[App] Auto-connect successful, session:', session.id);
        handleConnected(session.id);
      } else {
        console.log('[App] Auto-connect failed, showing dialog');
        setServerToConnect(server);
        setConnectForceNew(forceNew);
        setIsConnectOpen(true);
      }
    } else {
      console.log('[App] No saved credentials, opening connection dialog');
      setServerToConnect(server);
      setConnectForceNew(forceNew);
      setIsConnectOpen(true);
    }
  }, [connectWithCredentials, handleConnected]);

  const resolveServerForSession = useCallback(async (session: Session) => {
    let server = servers.find((candidate) =>
      candidate.id === session.serverId || candidate.name === session.serverName
    );

    if (server) {
      return server;
    }

    await fetchServers();
    server = useServerStore.getState().servers.find((candidate) =>
      candidate.id === session.serverId || candidate.name === session.serverName
    );

    return server ?? null;
  }, [servers, fetchServers]);

  const handleReconnectSession = useCallback(async (session: Session) => {
    if (session.sessionType !== 'ssh') {
      return;
    }

    const server = await resolveServerForSession(session);
    if (!server) {
      notifyWarning('Server Not Found', `Could not find a saved server for ${session.serverName}.`);
      return;
    }

    await closeInactiveSession(session);
    await handleConnect(server, { forceNew: true });
  }, [resolveServerForSession, notifyWarning, closeInactiveSession, handleConnect]);

  const handleAddServer = useCallback(() => {
    setIsAddServerOpen(true);
  }, []);

  const handleEditServer = useCallback((server: Server) => {
    setServerToEdit(server);
    setIsEditServerOpen(true);
  }, []);

  const handleNewSession = useCallback(() => {
    setIsSelectServerOpen(true);
  }, []);

  const handleNewSessionForServer = useCallback((server: Server) => {
    handleConnect(server, { forceNew: true });
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

    if (activeSession.sessionType === 'ssh' && activeSession.state !== 'connected') {
      notifyWarning('Session Disconnected', 'Reconnect the server before opening SFTP.');
      return;
    }

    sftpPanelRef.current?.expand();
  }, [activeSession, notifyWarning]);

  const handleData = useCallback((_data: string) => {
  }, []);

  const handleExpandSftp = useCallback(() => {
    console.log('Expand SFTP panel to full view clicked');
    notifyWarning('Coming Soon', 'Full SFTP file manager view is under development.');
  }, [notifyWarning]);

  const handleConfirmCloseSession = useCallback(async () => {
    if (!sessionToClose) return;

    const sessionId = sessionToClose;
    const session = sessions.find((s) => s.id === sessionId);
    setSessionToClose(null);

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
    <div className="app-shell h-screen flex flex-col bg-tokyo-bg">
      <div
        className="app-shell h-screen flex flex-col bg-tokyo-bg absolute inset-0 z-10"
        style={{ display: isSettingsView ? 'flex' : 'none' }}
      >
        <TitleBar />
        <Notifications />
        <header className="h-11 flex items-center px-4 bg-tokyo-bg-dark border-b border-tokyo-bg-hl">
          <button
            className={cn(
              'flex items-center gap-2 px-3 py-1.5 rounded-lg border border-transparent',
              'text-tokyo-fg hover:text-white hover:bg-tokyo-bg-hl',
              'hover:border-tokyo-selection transition-colors duration-150',
              'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
            )}
            onClick={goToMain}
          >
            <ArrowLeft className="w-4 h-4" />
            <span className="text-sm">{t('common.back')}</span>
          </button>
          <h1 className="ml-4 text-tokyo-fg font-semibold">{t('settings.title')}</h1>
        </header>
        <div className="flex-1 overflow-y-auto bg-tokyo-bg">
          {isSettingsView && (
            <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
              <Settings />
            </Suspense>
          )}
        </div>
      </div>

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
          <aside className="app-sidebar hidden w-64 flex-shrink-0 flex-col border-r border-tokyo-bg-hl bg-tokyo-bg-dark md:flex">
            <div className="flex-1 overflow-hidden">
              <ServerList
                onConnect={handleConnect}
                onAddServer={handleAddServer}
                onEditServer={handleEditServer}
                connectedServerIds={connectedServerIds}
                onNewSession={handleNewSessionForServer}
              />
            </div>

            <div className="border-t border-tokyo-bg-hl p-2 space-y-1.5">
              <div className="px-2 py-1 text-xs font-medium text-tokyo-comment">
                {t('common.actions')}
              </div>
              <SidebarAction
                icon={<Plus className="w-4 h-4" />}
                label={t('sidebar.addServer')}
                shortcut="Ctrl+N"
                variant="primary"
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

          <main className="main-workspace flex-1 flex flex-col min-w-0 relative">
            <SessionTabs
              onNewSession={handleNewSession}
              onReconnectSession={handleReconnectSession}
            />

            <div className="flex-1 min-h-0 flex flex-col">
              {sessions.length > 0 ? (
                <>
                  <div className="flex-1 min-h-0 relative">
                    {sessions.map((session) => (
                      <div
                        key={session.id}
                        className={
                          session.id === activeSessionId
                            ? 'terminal-card absolute inset-2'
                            : 'terminal-card absolute inset-2 invisible pointer-events-none'
                        }
                        style={
                          session.id !== activeSessionId
                            ? { height: 0, overflow: 'hidden' }
                            : undefined
                        }
                      >
                        <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                          <Terminal
                            ref={session.id === activeSessionId ? terminalRef : undefined}
                            sessionId={session.id}
                            onData={handleData}
                          />
                        </Suspense>
                      </div>
                    ))}
                  </div>
                  {activeSession && (
                    <ServerStatus
                      sessionId={activeSession.id}
                      defaultCollapsed={!settings.serverStatus.defaultExpanded}
                      defaultRefreshInterval={settings.serverStatus.refreshInterval}
                    />
                  )}
                </>
              ) : (
                <div className="h-full flex items-center justify-center bg-tokyo-bg p-6">
                  <div className="empty-session-panel text-center">
                    <div className="empty-session-console" aria-hidden="true">
                      <span />
                      <span />
                      <span />
                      <strong>VSH</strong>
                    </div>
                    <div className="empty-session-glyph mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl border border-tokyo-bg-hl bg-tokyo-bg-dark text-tokyo-blue">
                      <TerminalIcon className="h-7 w-7" aria-hidden="true" />
                    </div>
                    <p className="text-tokyo-fg text-lg font-semibold mb-2">{t('session.noActiveSession')}</p>
                    <p className="text-tokyo-comment text-sm mb-6">
                      {t('session.noActiveSessionDesc')}
                    </p>
                    <div className="empty-session-actions">
                      <button
                        className={cn(
                          'inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg',
                          'bg-tokyo-blue hover:bg-tokyo-cyan text-white',
                          'transition-colors duration-150',
                          'focus:outline-none focus:ring-2 focus:ring-tokyo-blue focus:ring-offset-2 focus:ring-offset-tokyo-bg'
                        )}
                        onClick={handleNewSession}
                      >
                        <Plus className="w-4 h-4" aria-hidden="true" />
                        {t('session.newSession')}
                      </button>
                      <button
                        className={cn(
                          'inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg border',
                          'border-tokyo-bg-hl bg-tokyo-bg text-tokyo-fg hover:bg-tokyo-bg-hl hover:text-white',
                          'transition-colors duration-150',
                          'focus:outline-none focus:ring-2 focus:ring-tokyo-blue focus:ring-offset-2 focus:ring-offset-tokyo-bg'
                        )}
                        onClick={handleAddServer}
                      >
                        <Plus className="w-4 h-4" aria-hidden="true" />
                        {t('sidebar.addServer')}
                      </button>
                    </div>
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

      <AddServerDialog
        isOpen={isAddServerOpen}
        onClose={() => setIsAddServerOpen(false)}
      />

      <EditServerDialog
        isOpen={isEditServerOpen}
        server={serverToEdit}
        onClose={() => {
          setIsEditServerOpen(false);
          setServerToEdit(null);
        }}
      />

      <ConnectDialog
        isOpen={isConnectOpen}
        server={serverToConnect}
        onClose={() => {
          setIsConnectOpen(false);
          setServerToConnect(null);
          setConnectForceNew(false);
        }}
        forceNew={connectForceNew}
        onConnected={handleConnected}
      />

      <QuickCommandDialog
        isOpen={isQuickCommandOpen}
        onClose={() => setIsQuickCommandOpen(false)}
      />

      <SelectServerDialog
        isOpen={isSelectServerOpen}
        onClose={() => setIsSelectServerOpen(false)}
        onSelectServer={handleConnect}
        onAddServer={handleAddServer}
        connectedServerIds={connectedServerIds}
      />

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

      <FingerprintVerificationDialog />

      <FingerprintManagerDialog />

      <SnippetManagerDialog
        isOpen={isSnippetManagerOpen}
        onClose={() => setIsSnippetManagerOpen(false)}
      />

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
