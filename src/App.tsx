import { useCallback, useState, useRef, useEffect, useMemo, lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Zap,
  FolderOpen,
  Settings as SettingsIcon,
  ArrowLeft,
  Terminal as TerminalIcon,
  ArrowRightLeft,
  Columns2,
  LayoutGrid,
  Rows2,
  PanelRightClose,
  Loader2,
  X,
  Bot,
  Blocks,
  Store,
} from 'lucide-react';
import { cn } from './lib/utils';
import { safeInvoke } from './lib/tauri';
import { useSessionStore, type Session } from './stores/sessionStore';
import { useNavigationStore } from './stores/navigationStore';
import { useNotificationStore } from './stores/notificationStore';
import { useSettingsStore, themes } from './stores/settingsStore';
import { UPDATE_CHECK_INTERVAL_MS, useUpdateStore } from './stores/updateStore';
import { SessionTabs } from './components/SessionTabs';
import { TitleBar } from './components/TitleBar';
import { SftpPanel, SftpPanelHandle } from './components/SftpPanel';
import { AddServerDialog } from './components/AddServerDialog';
import { EditServerDialog } from './components/EditServerDialog';
import { ConnectDialog } from './components/ConnectDialog';
import { SelectServerDialog } from './components/SelectServerDialog';
import { QuickCommandDialog } from './components/QuickCommandDialog';
import { ConfirmDialog } from './components/ConfirmDialog';
import { Notifications } from './components/Notifications';
import { AgentActivityPanel } from './components/AgentActivityPanel';
import { FingerprintVerificationDialog, FingerprintManagerDialog } from './components/FingerprintDialog';
import { SnippetManagerDialog } from './components/SnippetManager/SnippetManagerDialog';
import { TunnelPanelDialog } from './components/TunnelPanel/TunnelPanelDialog';
import { useServerStore, type Server } from './stores/serverStore';
import { usePluginStore } from './stores/pluginStore';
import { SessionPluginDock } from './components/SessionPluginDock';
import type { TerminalHandle } from './components/Terminal';
import {
  MAX_TERMINAL_PANES,
  addTerminalPane,
  getTerminalGridTracks,
  removeTerminalPane,
  syncTerminalPanes,
  type TerminalPaneLayout,
} from './lib/splitPanes';

const Settings = lazy(() => import('./components/Settings').then((mod) => ({ default: mod.Settings })));
const PluginMarketplace = lazy(() => import('./components/PluginMarketplace').then((mod) => ({ default: mod.PluginMarketplace })));
const Terminal = lazy(() => import('./components/Terminal').then((mod) => ({ default: mod.Terminal })));

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
    createLocalShellSession,
  } = useSessionStore();
  const { currentView, goToMain, goToSettings, goToPlugins } = useNavigationStore();
  const { warning: notifyWarning, error: notifyError } = useNotificationStore();
  const { settings, initializeSettings } = useSettingsStore();
  const { checkForUpdates, markVersionNotified } = useUpdateStore();
  const servers = useServerStore((state) => state.servers);
  const fetchServers = useServerStore((state) => state.fetchServers);
  const fetchGroups = useServerStore((state) => state.fetchGroups);
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);

  const [isAddServerOpen, setIsAddServerOpen] = useState(false);
  const [isConnectOpen, setIsConnectOpen] = useState(false);
  const [isQuickCommandOpen, setIsQuickCommandOpen] = useState(false);
  const [isEditServerOpen, setIsEditServerOpen] = useState(false);
  const [isSelectServerOpen, setIsSelectServerOpen] = useState(false);
  const [isSnippetManagerOpen, setIsSnippetManagerOpen] = useState(false);
  const [isTunnelPanelOpen, setIsTunnelPanelOpen] = useState(false);
  const [isSftpOpen, setIsSftpOpen] = useState(false);
  const [isAgentActivityOpen, setIsAgentActivityOpen] = useState(false);
  const [isPluginDockOpen, setIsPluginDockOpen] = useState(false);
  const [serverToConnect, setServerToConnect] = useState<Server | null>(null);
  const [connectForceNew, setConnectForceNew] = useState(false);
  const [serverToEdit, setServerToEdit] = useState<Server | null>(null);
  const [sessionToClose, setSessionToClose] = useState<string | null>(null);
  const [terminalPaneIds, setTerminalPaneIds] = useState<string[]>([]);
  const [terminalPaneLayout, setTerminalPaneLayout] = useState<TerminalPaneLayout>('grid');
  const [isCreatingTerminalPane, setIsCreatingTerminalPane] = useState(false);

  const terminalRef = useRef<TerminalHandle>(null);
  const sftpPanelRef = useRef<SftpPanelHandle>(null);
  const sessionBootstrapRef = useRef<Promise<void> | null>(null);
  const terminalPaneCreationRef = useRef(false);

  useEffect(() => {
    initializeSettings();
  }, [initializeSettings]);

  useEffect(() => {
    void fetchPlugins();
  }, [fetchPlugins]);

  useEffect(() => {
    void Promise.all([fetchServers(), fetchGroups()]);
  }, [fetchServers, fetchGroups]);

  useEffect(() => {
    let cancelled = false;

    const checkUpdates = async () => {
      const release = await checkForUpdates();
      if (!release || cancelled) return;

      const { lastNotifiedVersion } = useUpdateStore.getState();
      if (lastNotifiedVersion === release.version) return;

      notifyWarning(
        t('updates.availableTitle'),
        t('updates.availableMessage', { version: release.version }),
        12000
      );
      markVersionNotified(release.version);
    };

    checkUpdates();
    const intervalId = window.setInterval(checkUpdates, UPDATE_CHECK_INTERVAL_MS);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [checkForUpdates, markVersionNotified, notifyWarning, t]);

  useEffect(() => {
    if (!sessionBootstrapRef.current) {
      sessionBootstrapRef.current = (async () => {
        await fetchSessions();
        if (useSessionStore.getState().sessions.length === 0) {
          await createLocalShellSession(undefined, 80, 24);
        }
      })();
    }

    const intervalId = window.setInterval(() => {
      void syncRemoteSessions();
    }, 2000);

    return () => {
      window.clearInterval(intervalId);
    };
  }, [createLocalShellSession, fetchSessions, syncRemoteSessions]);

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
    root.style.setProperty('--tokyo-on-accent', currentTheme.colors.onAccent);
    root.dataset.theme = currentTheme.name;
    root.style.colorScheme = currentTheme.name === 'paper-white' || currentTheme.name === 'warm-ivory'
      ? 'light'
      : 'dark';

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

  const visiblePaneIds = useMemo(
    () => syncTerminalPanes(terminalPaneIds, sessions.map((session) => session.id), activeSessionId),
    [activeSessionId, sessions, terminalPaneIds]
  );

  const terminalGridTracks = useMemo(
    () => getTerminalGridTracks(terminalPaneLayout, visiblePaneIds.length),
    [terminalPaneLayout, visiblePaneIds.length]
  );

  useEffect(() => {
    setTerminalPaneIds((current) => {
      const next = syncTerminalPanes(current, sessions.map((session) => session.id), activeSessionId);
      return current.length === next.length && current.every((id, index) => id === next[index])
        ? current
        : next;
    });
  }, [activeSessionId, sessions]);

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

  const handleAddTerminalPane = useCallback(async () => {
    if (terminalPaneCreationRef.current) return;

    const storeState = useSessionStore.getState();
    const basePanes = syncTerminalPanes(
      terminalPaneIds,
      storeState.sessions.map((session) => session.id),
      storeState.activeSessionId
    );
    if (basePanes.length >= MAX_TERMINAL_PANES) {
      notifyWarning(t('session.splitLimitTitle'), t('session.splitLimitMessage'));
      return;
    }

    terminalPaneCreationRef.current = true;
    setIsCreatingTerminalPane(true);

    try {
      const sourceSession = storeState.sessions.find((session) => session.id === storeState.activeSessionId);
      const shellId = sourceSession?.sessionType === 'local' ? sourceSession.serverId : undefined;
      const session = await createLocalShellSession(shellId, 80, 24);
      if (!session) return;

      setTerminalPaneIds((current) => {
        const latestSessions = useSessionStore.getState().sessions.map((item) => item.id);
        return addTerminalPane(
          syncTerminalPanes(current, latestSessions, storeState.activeSessionId),
          session.id
        );
      });
      setActiveSession(session.id);
    } catch (error) {
      console.error('[App] Failed to create terminal pane:', error);
      notifyError(
        t('session.splitFailedTitle'),
        error instanceof Error ? error.message : t('session.splitFailedMessage')
      );
    } finally {
      terminalPaneCreationRef.current = false;
      setIsCreatingTerminalPane(false);
    }
  }, [createLocalShellSession, notifyError, notifyWarning, setActiveSession, t, terminalPaneIds]);

  const handleRemoveTerminalPane = useCallback((sessionId: string) => {
    const nextActiveSessionId = visiblePaneIds.find((id) => id !== sessionId) ?? null;
    setTerminalPaneIds((current) => removeTerminalPane(current, sessionId));
    if (activeSessionId === sessionId && nextActiveSessionId) {
      setActiveSession(nextActiveSessionId);
    }
  }, [activeSessionId, setActiveSession, visiblePaneIds]);

  const handleCollapseTerminalPanes = useCallback(() => {
    if (activeSessionId) setTerminalPaneIds([activeSessionId]);
  }, [activeSessionId]);

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

    if (!isSftpOpen) {
      setIsAgentActivityOpen(false);
    }
    sftpPanelRef.current?.toggle();
  }, [activeSession, isSftpOpen, notifyWarning]);

  const handleOpenAgentActivity = useCallback(() => {
    setIsAgentActivityOpen((current) => {
      const next = !current;
      if (next && isSftpOpen) {
        sftpPanelRef.current?.toggle();
      }
      return next;
    });
  }, [isSftpOpen]);

  const handleSftpCollapsedChange = useCallback((collapsed: boolean) => {
    setIsSftpOpen(!collapsed);
  }, []);

  const handleGatewaySessionsChanged = useCallback(() => {
    void syncRemoteSessions();
  }, [syncRemoteSessions]);

  const handleData = useCallback((_data: string) => {
  }, []);

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
  const isPluginsView = currentView === 'plugins';
  const isOverlayView = currentView !== 'main';
  const sessionToCloseObj = sessionToClose ? sessions.find((s) => s.id === sessionToClose) : null;

  return (
    <div className="app-shell h-screen flex flex-col bg-tokyo-bg">
      <div
        className="app-shell h-screen flex flex-col bg-tokyo-bg absolute inset-0 z-10"
        style={{ display: isOverlayView ? 'flex' : 'none' }}
      >
        <TitleBar />
        <Notifications />
        <header className="h-11 flex items-center px-4 bg-tokyo-bg-dark border-b border-tokyo-bg-hl">
          <button
            className={cn(
              'flex items-center gap-2 px-3 py-1.5 rounded-lg border border-transparent',
              'text-tokyo-fg hover:text-tokyo-fg hover:bg-tokyo-bg-hl',
              'hover:border-tokyo-selection transition-colors duration-150',
              'focus:outline-none focus:ring-1 focus:ring-tokyo-blue'
            )}
            onClick={goToMain}
          >
            <ArrowLeft className="w-4 h-4" />
            <span className="text-sm">{t('common.back')}</span>
          </button>
          <h1 className="ml-4 text-tokyo-fg font-semibold">
            {isPluginsView ? t('plugins.marketplace') : t('settings.title')}
          </h1>
        </header>
        <div className="flex-1 overflow-y-auto bg-tokyo-bg">
          {isSettingsView && (
            <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
              <Settings />
            </Suspense>
          )}
          {isPluginsView && (
            <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
              <PluginMarketplace />
            </Suspense>
          )}
        </div>
      </div>

      <div
        className="h-full flex flex-col flex-1"
        style={{ visibility: isOverlayView ? 'hidden' : 'visible' }}
      >
        <TitleBar
          activeSessionName={activeSession?.serverName}
          activeSessionType={activeSession?.sessionType}
          activeSessionState={activeSession?.state}
        />
        <Notifications />

        <div className="flex-1 flex overflow-hidden">
          <main className="main-workspace flex-1 flex flex-col min-w-0 relative overflow-x-hidden w-full max-w-full">
            <SessionTabs
              onNewSession={handleNewSession}
              onReconnectSession={handleReconnectSession}
              rightActions={(
                <div className="flex flex-shrink-0 items-center gap-0.5 border-l border-tokyo-bg-hl pl-1.5">
                  <button className="icon-button tooltip-button" data-tooltip={`${t('sidebar.quickCmd')} (Ctrl+K)`} onClick={handleQuickCommand} aria-label={t('sidebar.quickCmd')}>
                    <Zap className="h-4 w-4" />
                  </button>
                  <button className="icon-button tooltip-button" data-tooltip={t('sidebar.tunnels')} onClick={handleOpenTunnels} aria-label={t('sidebar.tunnels')}>
                    <ArrowRightLeft className="h-4 w-4" />
                  </button>
                  <button className="icon-button tooltip-button" data-tooltip={t('sidebar.snippets')} onClick={handleOpenSnippets} aria-label={t('sidebar.snippets')}>
                    <TerminalIcon className="h-4 w-4" />
                  </button>
                  <button
                    className={cn('icon-button tooltip-button', isAgentActivityOpen && 'is-active')}
                    data-tooltip={t('agentActivity.title')}
                    onClick={handleOpenAgentActivity}
                    aria-pressed={isAgentActivityOpen}
                    aria-label={t('agentActivity.title')}
                  >
                    <Bot className="h-4 w-4" />
                  </button>
                  <button
                    className={cn('icon-button tooltip-button', isSftpOpen && 'is-active')}
                    data-tooltip={t('sidebar.sftp')}
                    onClick={handleOpenSftp}
                    disabled={!activeSession}
                    aria-pressed={isSftpOpen}
                    aria-label={t('sidebar.sftp')}
                  >
                    <FolderOpen className="h-4 w-4" />
                  </button>
                  <button
                    className="icon-button tooltip-button"
                    data-tooltip={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitTerminal')}
                    onClick={() => { void handleAddTerminalPane(); }}
                    disabled={isCreatingTerminalPane}
                    aria-label={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitTerminal')}
                  >
                    {isCreatingTerminalPane
                      ? <Loader2 className="h-4 w-4 animate-spin" />
                      : <Columns2 className="h-4 w-4" />}
                  </button>
                  {visiblePaneIds.length > 1 && (
                    <>
                      <button
                        className={cn('icon-button tooltip-button', terminalPaneLayout === 'grid' && 'is-active')}
                        data-tooltip={t('session.arrangeGrid')}
                        onClick={() => setTerminalPaneLayout('grid')}
                        aria-label={t('session.arrangeGrid')}
                        aria-pressed={terminalPaneLayout === 'grid'}
                      >
                        <LayoutGrid className="h-4 w-4" />
                      </button>
                      <button
                        className={cn('icon-button tooltip-button', terminalPaneLayout === 'columns' && 'is-active')}
                        data-tooltip={t('session.arrangeColumns')}
                        onClick={() => setTerminalPaneLayout('columns')}
                        aria-label={t('session.arrangeColumns')}
                        aria-pressed={terminalPaneLayout === 'columns'}
                      >
                        <Columns2 className="h-4 w-4" />
                      </button>
                      <button
                        className={cn('icon-button tooltip-button', terminalPaneLayout === 'rows' && 'is-active')}
                        data-tooltip={t('session.arrangeRows')}
                        onClick={() => setTerminalPaneLayout('rows')}
                        aria-label={t('session.arrangeRows')}
                        aria-pressed={terminalPaneLayout === 'rows'}
                      >
                        <Rows2 className="h-4 w-4" />
                      </button>
                      <button
                        className="icon-button tooltip-button"
                        data-tooltip={t('session.closeSplits')}
                        onClick={handleCollapseTerminalPanes}
                        aria-label={t('session.closeSplits')}
                      >
                        <PanelRightClose className="h-4 w-4" />
                      </button>
                    </>
                  )}
                  <button
                    className={cn('icon-button tooltip-button', isPluginDockOpen && 'is-active')}
                    data-tooltip={t('plugins.workspace')}
                    onClick={() => setIsPluginDockOpen((open) => !open)}
                    disabled={!activeSession}
                    aria-pressed={isPluginDockOpen}
                    aria-label={t('plugins.workspace')}
                  >
                    <Blocks className="h-4 w-4" />
                  </button>
                  <button
                    className="icon-button tooltip-button"
                    data-tooltip={t('plugins.marketplace')}
                    onClick={goToPlugins}
                    aria-label={t('plugins.marketplace')}
                  >
                    <Store className="h-4 w-4" />
                  </button>
                  <button className="icon-button tooltip-button" data-tooltip={`${t('sidebar.settings')} (Ctrl+,)`} onClick={goToSettings} aria-label={t('sidebar.settings')}>
                    <SettingsIcon className="h-4 w-4" />
                  </button>
                </div>
              )}
            />

            <div className="flex-1 min-h-0 flex">
              <div className="flex min-w-0 flex-1 flex-col">
                {sessions.length > 0 ? (
                  <>
                    <div
                      className="relative grid min-h-0 flex-1 gap-2 p-2"
                      style={{
                        gridTemplateColumns: `repeat(${terminalGridTracks.columns}, minmax(0, 1fr))`,
                        gridTemplateRows: `repeat(${terminalGridTracks.rows}, minmax(0, 1fr))`,
                      }}
                    >
                    {sessions.map((session) => (
                      <div
                        key={session.id}
                        className={cn(
                          'terminal-card min-h-0 min-w-0 flex-col',
                          visiblePaneIds.includes(session.id)
                            ? 'relative flex'
                            : 'pointer-events-none invisible absolute h-0 w-0 overflow-hidden',
                          visiblePaneIds.length > 1 && session.id === activeSessionId && 'border-tokyo-cyan'
                        )}
                        onMouseDown={() => {
                          if (visiblePaneIds.includes(session.id) && session.id !== activeSessionId) {
                            setActiveSession(session.id);
                          }
                        }}
                      >
                        {visiblePaneIds.length > 1 && visiblePaneIds.includes(session.id) && (
                          <div className="flex h-7 flex-shrink-0 items-center gap-2 border-b border-tokyo-bg-hl bg-tokyo-bg-dark px-2">
                            <span className={cn(
                              'h-1.5 w-1.5 rounded-full',
                              session.id === activeSessionId ? 'bg-tokyo-cyan' : 'bg-tokyo-comment'
                            )} />
                            <span className="min-w-0 flex-1 truncate text-[11px] text-tokyo-fg">{session.serverName}</span>
                            <button
                              className="icon-button h-5 w-5"
                              onMouseDown={(event) => event.stopPropagation()}
                              onClick={() => handleRemoveTerminalPane(session.id)}
                              aria-label={t('session.removePane', { name: session.serverName })}
                              title={t('session.removePane', { name: session.serverName })}
                            >
                              <X className="h-3 w-3" />
                            </button>
                          </div>
                        )}
                        <div className="relative min-h-0 flex-1">
                          <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                            <Terminal
                              ref={session.id === activeSessionId ? terminalRef : undefined}
                              sessionId={session.id}
                              onData={handleData}
                            />
                          </Suspense>
                        </div>
                      </div>
                    ))}
                    </div>
                    {activeSession && (
                      <SessionPluginDock
                        sessionId={activeSession.id}
                        sessionType={activeSession.sessionType}
                        open={isPluginDockOpen}
                        onClose={() => setIsPluginDockOpen(false)}
                        onOpenMarketplace={goToPlugins}
                      />
                    )}
                  </>
                ) : (
                  <div className="h-full bg-tokyo-bg" aria-label={t('session.localShell')} />
                )}
              </div>

              <SftpPanel
                ref={sftpPanelRef}
                sessionId={activeSession?.id}
                sessionType={activeSession?.sessionType}
                defaultCollapsed={true}
                dock="right"
                onCollapsedChange={handleSftpCollapsedChange}
              />
              <AgentActivityPanel
                open={isAgentActivityOpen}
                onClose={() => setIsAgentActivityOpen(false)}
                onSessionsChanged={handleGatewaySessionsChanged}
              />
            </div>
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
        onEditServer={handleEditServer}
        onNewSession={handleNewSessionForServer}
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
