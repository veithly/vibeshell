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
  Rows2,
  PanelRightClose,
  Loader2,
  X,
  Bot,
  Code2,
  FileDiff,
  Blocks,
  Store,
} from 'lucide-react';
import { Mosaic, MosaicWindow, type MosaicNode, type MosaicBranch } from 'react-mosaic-component2';
import 'react-mosaic-component2/react-mosaic-component.css';
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
import { WorkspaceChangesPanel } from './components/WorkspaceChangesPanel';
import { MobileWorkspaceActions } from './components/MobileWorkspaceActions';
import { WorkspaceToolbar } from './components/WorkspaceToolbar';
import { FingerprintVerificationDialog, FingerprintManagerDialog } from './components/FingerprintDialog';
import { SnippetManagerDialog } from './components/SnippetManager/SnippetManagerDialog';
import { TunnelPanelDialog } from './components/TunnelPanel/TunnelPanelDialog';
import { useServerStore, type Server } from './stores/serverStore';
import { useRuntimeCapabilitiesStore } from './stores/runtimeCapabilitiesStore';
import { useMediaQuery } from './lib/useMediaQuery';
import { usePluginStore } from './stores/pluginStore';
import { useFileWorkspaceStore } from './stores/fileWorkspaceStore';
import { SessionPluginDock } from './components/SessionPluginDock';
import type { TerminalHandle } from './components/Terminal';
import {
  MAX_TERMINAL_PANES,
  addPane,
  removePane,
  pruneLeaves,
  getLeaves,
  countLeaves,
} from './lib/mosaicTree';

const Settings = lazy(() => import('./components/Settings').then((mod) => ({ default: mod.Settings })));
const PluginMarketplace = lazy(() => import('./components/PluginMarketplace').then((mod) => ({ default: mod.PluginMarketplace })));
const Terminal = lazy(() => import('./components/Terminal').then((mod) => ({ default: mod.Terminal })));
const FileWorkspace = lazy(() => import('./components/FileWorkspace').then((mod) => ({ default: mod.FileWorkspace })));

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
  const runtimeCapabilities = useRuntimeCapabilitiesStore((state) => state.capabilities);
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);
  const isCompactWorkspace = useMediaQuery('(max-width: 767px)');
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);
  const fileTabs = useFileWorkspaceStore((state) => state.tabs);
  const activeFileTabId = useFileWorkspaceStore((state) => state.activeTabId);
  const activateFileTab = useFileWorkspaceStore((state) => state.activateTab);
  const closeFileTab = useFileWorkspaceStore((state) => state.closeTab);

  const [isAddServerOpen, setIsAddServerOpen] = useState(false);
  const [isConnectOpen, setIsConnectOpen] = useState(false);
  const [isQuickCommandOpen, setIsQuickCommandOpen] = useState(false);
  const [isEditServerOpen, setIsEditServerOpen] = useState(false);
  const [isSelectServerOpen, setIsSelectServerOpen] = useState(false);
  const [sessionLauncherTab, setSessionLauncherTab] = useState<'agent' | 'local' | 'ssh' | undefined>();
  const [isSnippetManagerOpen, setIsSnippetManagerOpen] = useState(false);
  const [isTunnelPanelOpen, setIsTunnelPanelOpen] = useState(false);
  const [isSftpOpen, setIsSftpOpen] = useState(false);
  const [isAgentActivityOpen, setIsAgentActivityOpen] = useState(false);
  const [isWorkspaceChangesOpen, setIsWorkspaceChangesOpen] = useState(false);
  const [isPluginDockOpen, setIsPluginDockOpen] = useState(false);
  const [serverToConnect, setServerToConnect] = useState<Server | null>(null);
  const [connectForceNew, setConnectForceNew] = useState(false);
  const [serverToEdit, setServerToEdit] = useState<Server | null>(null);
  const [sessionToClose, setSessionToClose] = useState<string | null>(null);
  const [mosaicTree, setMosaicTree] = useState<MosaicNode<string> | null>(null);
  const [isCreatingTerminalPane, setIsCreatingTerminalPane] = useState(false);

  // Per-pane terminal handles. Mosaic can render multiple panes at once, so a
  // single ref is insufficient; each pane registers/unregisters via callback ref.
  const terminalRefs = useRef<Map<string, TerminalHandle>>(new Map());
  const sftpPanelRef = useRef<SftpPanelHandle>(null);
  const sessionBootstrapRef = useRef<Promise<void> | null>(null);
  const terminalPaneCreationRef = useRef(false);

  useEffect(() => {
    initializeSettings();
  }, [initializeSettings]);

  useEffect(() => {
    void loadRuntimeCapabilities();
  }, [loadRuntimeCapabilities]);

  useEffect(() => {
    void fetchPlugins();
  }, [fetchPlugins]);

  useEffect(() => {
    void Promise.all([fetchServers(), fetchGroups()]);
  }, [fetchServers, fetchGroups]);

  useEffect(() => {
    if (!runtimeCapabilities.desktopUpdater) return;

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
  }, [checkForUpdates, markVersionNotified, notifyWarning, runtimeCapabilities.desktopUpdater, t]);

  useEffect(() => {
    if (!sessionBootstrapRef.current) {
      sessionBootstrapRef.current = (async () => {
        const capabilities = await loadRuntimeCapabilities();
        await fetchSessions();
        if (capabilities.localShell && useSessionStore.getState().sessions.length === 0) {
          await createLocalShellSession(undefined, 80, 24);
        }
      })();
    }

    // Poll session state every 2s, but pause while the window is hidden to
    // avoid pointless IPC when the app is in the background. On regaining
    // visibility, sync immediately so stale UI refreshes without waiting.
    let intervalId: number | null = window.setInterval(() => {
      if (document.hidden) return;
      void syncRemoteSessions();
    }, 2000);

    const handleVisibilityChange = () => {
      if (document.hidden) {
        if (intervalId !== null) {
          window.clearInterval(intervalId);
          intervalId = null;
        }
      } else {
        void syncRemoteSessions();
        if (intervalId === null) {
          intervalId = window.setInterval(() => {
            if (document.hidden) return;
            void syncRemoteSessions();
          }, 2000);
        }
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      if (intervalId !== null) {
        window.clearInterval(intervalId);
      }
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [createLocalShellSession, fetchSessions, loadRuntimeCapabilities, syncRemoteSessions]);

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
    root.style.setProperty('--tokyo-red', currentTheme.colors.red);
    root.style.setProperty('--tokyo-green', currentTheme.colors.green);
    root.style.setProperty('--tokyo-yellow', currentTheme.colors.yellow);
    root.style.setProperty('--tokyo-magenta', currentTheme.colors.magenta);
    root.style.setProperty('--tokyo-cyan', currentTheme.colors.cyan);
    root.style.setProperty('--tokyo-orange', currentTheme.colors.orange);
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

      const isCtrl = event.ctrlKey || event.metaKey;
      if (isCtrl && event.key.toLowerCase() === 'w' && activeFileTabId) {
        event.preventDefault();
        const fileTab = fileTabs.find((tab) => tab.id === activeFileTabId);
        if (!fileTab?.dirty || window.confirm(t('fileWorkspace.discardChanges'))) {
          closeFileTab(activeFileTabId);
        }
        return;
      }

      const target = event.target as HTMLElement;
      if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
        return;
      }

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
              const hasUnsavedFiles = fileTabs.some(
                (tab) => tab.sessionId === activeSessionId && tab.dirty
              );
              if (activeSession?.state === 'connected' || activeSession?.state === 'connecting' || hasUnsavedFiles) {
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
  }, [activeFileTabId, activeSessionId, closeFileTab, closeInactiveSession, fileTabs, goToSettings, sessions, t]);

  const connectedServerIds = useMemo(() => new Set(
    sessions
      .filter((s) => s.state === 'connected' || s.state === 'connecting')
      .map((s) => s.serverId)
  ), [sessions]);

  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId),
    [sessions, activeSessionId]
  );
  const activeFileTab = useMemo(
    () => fileTabs.find((tab) => tab.id === activeFileTabId) ?? null,
    [activeFileTabId, fileTabs]
  );

  useEffect(() => {
    if (
      activeFileTab
      && activeSessionId !== activeFileTab.sessionId
      && sessions.some((session) => session.id === activeFileTab.sessionId)
    ) {
      setActiveSession(activeFileTab.sessionId);
    }
  }, [activeFileTab, activeSessionId, sessions, setActiveSession]);

  useEffect(() => {
    if (activeSession?.purpose !== 'coding_agent') {
      setIsWorkspaceChangesOpen(false);
    }
  }, [activeSession?.purpose]);

  // A selected tab must always reveal its terminal. Keep an existing split when
  // the active session is already one of its panes; otherwise switch to it.
  useEffect(() => {
    if (!activeSessionId) return;
    setMosaicTree((current) => {
      if (current !== null && getLeaves(current).includes(activeSessionId)) {
        return current;
      }
      return activeSessionId;
    });
  }, [activeSessionId]);

  // Prune panes whose sessions have been closed, keeping the layout otherwise intact.
  useEffect(() => {
    setMosaicTree((current) => {
      if (current === null) return current;
      const validIds = new Set(sessions.map((session) => session.id));
      const pruned = pruneLeaves(current, validIds);
      // If everything was pruned, fall back to the active session (or null).
      if (pruned === null) {
        return activeSessionId ?? null;
      }
      return pruned === current ? current : pruned;
    });
  }, [sessions, activeSessionId]);

  const handleConnected = useCallback((sessionId: string) => {
    console.log('[App] handleConnected called with sessionId:', sessionId);
    activateFileTab(null);
    setActiveSession(sessionId);

    // The Terminal component attaches after its event listener is ready so
    // the initial prompt/MOTD can be replayed without losing early output.
    setTimeout(() => {
      console.log('[App] Focusing terminal for session:', sessionId);
      terminalRefs.current.get(sessionId)?.focus();
    }, 100);
  }, [activateFileTab, setActiveSession]);

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
    setSessionLauncherTab(undefined);
    setIsSelectServerOpen(true);
  }, []);

  const handleOpenCodingAgent = useCallback(() => {
    setSessionLauncherTab('agent');
    setIsSelectServerOpen(true);
  }, []);

  const handleCodingAgentLaunched = useCallback((sessionId: string) => {
    activateFileTab(null);
    setActiveSession(sessionId);
    setMosaicTree(sessionId);
    window.setTimeout(() => terminalRefs.current.get(sessionId)?.focus(), 100);
  }, [activateFileTab, setActiveSession]);

  const handleSplitPane = useCallback(async (direction: 'row' | 'column') => {
    if (terminalPaneCreationRef.current) return;

    const storeState = useSessionStore.getState();
    const currentTree = mosaicTree;
    if (currentTree !== null && countLeaves(currentTree) >= MAX_TERMINAL_PANES) {
      notifyWarning(t('session.splitLimitTitle'), t('session.splitLimitMessage'));
      return;
    }

    const targetId = storeState.activeSessionId;
    if (!targetId) return;

    terminalPaneCreationRef.current = true;
    setIsCreatingTerminalPane(true);

    try {
      const sourceSession = storeState.sessions.find((session) => session.id === targetId);
      const shellId = sourceSession?.sessionType === 'local' && sourceSession.purpose !== 'coding_agent'
        ? sourceSession.serverId
        : undefined;
      const session = await createLocalShellSession(shellId, 80, 24);
      if (!session) return;

      setMosaicTree((current) => addPane(current, targetId, session.id, direction));
      // Keep the current pane active so repeated splits build outward from the
      // same origin pane instead of chaining off each newly-created pane.
      // The new pane is still immediately usable — clicking it focuses it.
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
  }, [createLocalShellSession, mosaicTree, notifyError, notifyWarning, setActiveSession, t]);

  const handleRemoveTerminalPane = useCallback((sessionId: string) => {
    setMosaicTree((current) => {
      if (countLeaves(current) <= 1) return current;

      const next = removePane(current, sessionId);
      // Pick a new active session from the remaining leaves.
      const remaining = getLeaves(next);
      if (activeSessionId === sessionId) {
        const nextActive = remaining[0] ?? null;
        if (nextActive) setActiveSession(nextActive);
      }
      return next;
    });
  }, [activeSessionId, setActiveSession]);

  const handleCollapseTerminalPanes = useCallback(() => {
    if (activeSessionId) setMosaicTree(activeSessionId);
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
      setIsWorkspaceChangesOpen(false);
    }
    sftpPanelRef.current?.toggle();
  }, [activeSession, isSftpOpen, notifyWarning]);

  const handleOpenAgentActivity = useCallback(() => {
    setIsAgentActivityOpen((current) => {
      const next = !current;
      if (next && isSftpOpen) {
        sftpPanelRef.current?.toggle();
      }
      if (next) {
        setIsWorkspaceChangesOpen(false);
      }
      return next;
    });
  }, [isSftpOpen]);

  const handleOpenWorkspaceChanges = useCallback(() => {
    if (activeSession?.purpose !== 'coding_agent' || !activeSession.cwd) {
      notifyWarning(t('workspaceChanges.title'), t('workspaceChanges.noWorkspace'));
      return;
    }

    setIsWorkspaceChangesOpen((current) => {
      const next = !current;
      if (next) {
        setIsAgentActivityOpen(false);
        if (isSftpOpen) sftpPanelRef.current?.toggle();
      }
      return next;
    });
  }, [activeSession, isSftpOpen, notifyWarning, t]);

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
  const sessionToCloseDirtyFileCount = sessionToClose
    ? fileTabs.filter((tab) => tab.sessionId === sessionToClose && tab.dirty).length
    : 0;
  const sessionToCloseMessage = sessionToCloseObj?.purpose === 'coding_agent'
    ? t('codingAgent.closeConfirm', { name: sessionToCloseObj?.serverName })
    : sessionToCloseObj?.sessionType === 'local'
      ? t('session.closeLocalShellConfirm', { name: sessionToCloseObj?.serverName })
      : t('session.closeSessionConfirm', { name: sessionToCloseObj?.serverName });
  const canRemoveTerminalPane = countLeaves(mosaicTree) > 1;

  return (
    <div className="app-shell h-screen flex flex-col bg-tokyo-bg">
      <TitleBar
        activeSessionName={activeSession?.serverName}
        activeSessionType={activeSession?.sessionType}
        activeSessionState={activeSession?.state}
        activeSessionPurpose={activeSession?.purpose}
      />
      <div
        className="app-shell absolute inset-x-0 bottom-0 top-9 z-10 flex flex-col bg-tokyo-bg"
        style={{ display: isOverlayView ? 'flex' : 'none' }}
      >
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
        <Notifications />

        <div className="flex-1 flex overflow-hidden">
          <main className="main-workspace flex-1 flex flex-col min-w-0 relative overflow-x-hidden w-full max-w-full">
            <SessionTabs
              onNewSession={handleNewSession}
              onReconnectSession={handleReconnectSession}
              rightActions={(
                runtimeCapabilities.isMobile || isCompactWorkspace ? (
                  <MobileWorkspaceActions
                    isSftpOpen={isSftpOpen}
                    sftpDisabled={!activeSession}
                    labels={{
                      sftp: t('sidebar.sftp'),
                      more: t('common.more'),
                    }}
                    menuItems={[
                      {
                        id: 'quick-command',
                        label: t('sidebar.quickCmd'),
                        icon: <Zap className="h-4 w-4" />,
                        onSelect: handleQuickCommand,
                      },
                      {
                        id: 'snippets',
                        label: t('sidebar.snippets'),
                        icon: <TerminalIcon className="h-4 w-4" />,
                        onSelect: handleOpenSnippets,
                      },
                      ...(runtimeCapabilities.localShell ? [{
                        id: 'coding-agent',
                        label: t('codingAgent.start'),
                        icon: <Code2 className="h-4 w-4" />,
                        onSelect: handleOpenCodingAgent,
                      }] : []),
                      {
                        id: 'workspace-changes',
                        label: t('workspaceChanges.title'),
                        icon: <FileDiff className="h-4 w-4" />,
                        disabled: activeSession?.purpose !== 'coding_agent' || !activeSession.cwd,
                        pressed: isWorkspaceChangesOpen,
                        onSelect: handleOpenWorkspaceChanges,
                      },
                      ...(runtimeCapabilities.agentGateway ? [{
                        id: 'agent-activity',
                        label: t('agentActivity.title'),
                        icon: <Bot className="h-4 w-4" />,
                        pressed: isAgentActivityOpen,
                        onSelect: handleOpenAgentActivity,
                      }] : []),
                      {
                        id: 'plugin-workspace',
                        label: t('plugins.workspace'),
                        icon: <Blocks className="h-4 w-4" />,
                        disabled: !activeSession,
                        pressed: isPluginDockOpen,
                        onSelect: () => setIsPluginDockOpen((open) => !open),
                      },
                      {
                        id: 'plugin-marketplace',
                        label: t('plugins.marketplace'),
                        icon: <Store className="h-4 w-4" />,
                        onSelect: goToPlugins,
                      },
                      {
                        id: 'settings',
                        label: t('sidebar.settings'),
                        icon: <SettingsIcon className="h-4 w-4" />,
                        onSelect: goToSettings,
                      },
                    ]}
                    onToggleSftp={handleOpenSftp}
                  />
                ) : (
                  <div className="flex flex-shrink-0 items-center gap-0.5 border-l border-tokyo-bg-hl pl-1.5">
                  {/* High-frequency session actions stay as direct buttons */}
                  <button className="icon-button tooltip-button" data-tooltip={`${t('sidebar.quickCmd')} (Ctrl+K)`} onClick={handleQuickCommand} aria-label={t('sidebar.quickCmd')}>
                    <Zap className="h-4 w-4" />
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
                  {runtimeCapabilities.localShell && (
                    <>
                      <button
                        className="icon-button tooltip-button"
                        data-tooltip={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitHorizontal')}
                        onClick={() => { void handleSplitPane('row'); }}
                        disabled={isCreatingTerminalPane}
                        aria-label={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitHorizontal')}
                      >
                        {isCreatingTerminalPane
                          ? <Loader2 className="h-4 w-4 animate-spin" />
                          : <Columns2 className="h-4 w-4" />}
                      </button>
                      <button
                        className="icon-button tooltip-button"
                        data-tooltip={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitVertical')}
                        onClick={() => { void handleSplitPane('column'); }}
                        disabled={isCreatingTerminalPane}
                        aria-label={isCreatingTerminalPane ? t('session.creatingPane') : t('session.splitVertical')}
                      >
                        {isCreatingTerminalPane
                          ? <Loader2 className="h-4 w-4 animate-spin" />
                          : <Rows2 className="h-4 w-4" />}
                      </button>
                    </>
                  )}
                  {/* Lower-frequency actions collapse into an overflow menu */}
                  <WorkspaceToolbar
                    label={t('common.more')}
                    anyPressed={isWorkspaceChangesOpen || isAgentActivityOpen || isPluginDockOpen}
                    items={[
                      ...(runtimeCapabilities.backgroundTunnels ? [{
                        id: 'tunnels',
                        label: t('sidebar.tunnels'),
                        icon: <ArrowRightLeft className="h-4 w-4" />,
                        onSelect: handleOpenTunnels,
                      }] : []),
                      {
                        id: 'snippets',
                        label: t('sidebar.snippets'),
                        icon: <TerminalIcon className="h-4 w-4" />,
                        onSelect: handleOpenSnippets,
                      },
                      ...(runtimeCapabilities.localShell ? [{
                        id: 'coding-agent',
                        label: t('codingAgent.start'),
                        icon: <Code2 className="h-4 w-4" />,
                        onSelect: handleOpenCodingAgent,
                      }] : []),
                      {
                        id: 'workspace-changes',
                        label: t('workspaceChanges.title'),
                        icon: <FileDiff className="h-4 w-4" />,
                        disabled: activeSession?.purpose !== 'coding_agent' || !activeSession.cwd,
                        pressed: isWorkspaceChangesOpen,
                        onSelect: handleOpenWorkspaceChanges,
                      },
                      ...(runtimeCapabilities.agentGateway ? [{
                        id: 'agent-activity',
                        label: t('agentActivity.title'),
                        icon: <Bot className="h-4 w-4" />,
                        pressed: isAgentActivityOpen,
                        onSelect: handleOpenAgentActivity,
                      }] : []),
                      {
                        id: 'plugin-workspace',
                        label: t('plugins.workspace'),
                        icon: <Blocks className="h-4 w-4" />,
                        disabled: !activeSession,
                        pressed: isPluginDockOpen,
                        onSelect: () => setIsPluginDockOpen((open) => !open),
                      },
                      ...(runtimeCapabilities.localShell && mosaicTree !== null && countLeaves(mosaicTree) > 1 ? [{
                        id: 'close-splits',
                        label: t('session.closeSplits'),
                        icon: <PanelRightClose className="h-4 w-4" />,
                        onSelect: handleCollapseTerminalPanes,
                      }] : []),
                      {
                        id: 'plugin-marketplace',
                        label: t('plugins.marketplace'),
                        icon: <Store className="h-4 w-4" />,
                        onSelect: goToPlugins,
                      },
                      {
                        id: 'settings',
                        label: t('sidebar.settings'),
                        icon: <SettingsIcon className="h-4 w-4" />,
                        onSelect: goToSettings,
                      },
                    ]}
                  />
                  </div>
                )
              )}
            />

            <div className="relative flex min-h-0 flex-1">
              <div className="flex min-w-0 flex-1 flex-col">
                {fileTabs.map((tab) => (
                  <div key={tab.id} className={cn('min-h-0 flex-1', activeFileTabId === tab.id ? 'block' : 'hidden')}>
                    <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                      <FileWorkspace tab={tab} isActive={activeFileTabId === tab.id} />
                    </Suspense>
                  </div>
                ))}
                <div className={cn('min-h-0 flex-1 flex-col', activeFileTab ? 'hidden' : 'flex')}>
                  {sessions.length > 0 ? (
                    <>
                    <div className="mosaic-container relative min-h-0 flex-1 p-2">
                      <Mosaic<string>
                        value={mosaicTree}
                        onChange={(node) => setMosaicTree(node)}
                        renderTile={(id: string, path: MosaicBranch[]) => {
                          const session = sessions.find((s) => s.id === id);
                          const serverName = session?.serverName ?? t('session.localShell');
                          return (
                            <MosaicWindow<string>
                              path={path}
                              title={serverName}
                              toolbarControls={canRemoveTerminalPane ? [
                                <button
                                  key="close"
                                  className="icon-button h-5 w-5"
                                  onClick={() => handleRemoveTerminalPane(id)}
                                  aria-label={t('session.removePane', { name: serverName })}
                                  title={t('session.removePane', { name: serverName })}
                                >
                                  <X className="h-3 w-3" />
                                </button>,
                              ] : []}
                              className={cn(id === activeSessionId && 'mosaic-window-active')}
                            >
                              <div
                                className="relative h-full bg-tokyo-bg"
                                onMouseDown={() => {
                                  if (id !== activeSessionId) setActiveSession(id);
                                }}
                              >
                                <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                                  <Terminal
                                    ref={(handle: TerminalHandle | null) => {
                                      if (handle) {
                                        terminalRefs.current.set(id, handle);
                                      } else {
                                        terminalRefs.current.delete(id);
                                      }
                                    }}
                                    sessionId={id}
                                    onData={handleData}
                                  />
                                </Suspense>
                              </div>
                            </MosaicWindow>
                          );
                        }}
                      />
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
              </div>

              <SftpPanel
                ref={sftpPanelRef}
                sessionId={activeSession?.id}
                sessionType={activeSession?.sessionType}
                defaultCollapsed={true}
                dock="right"
                onCollapsedChange={handleSftpCollapsedChange}
              />
              <WorkspaceChangesPanel
                open={isWorkspaceChangesOpen}
                cwd={activeSession?.purpose === 'coding_agent' ? activeSession.cwd : undefined}
                sessionName={activeSession?.purpose === 'coding_agent' ? activeSession.serverName : undefined}
                onClose={() => setIsWorkspaceChangesOpen(false)}
              />
              {runtimeCapabilities.agentGateway && (
                <AgentActivityPanel
                  open={isAgentActivityOpen}
                  onClose={() => setIsAgentActivityOpen(false)}
                  onSessionsChanged={handleGatewaySessionsChanged}
                />
              )}
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
        initialTab={sessionLauncherTab}
        initialWorkspace={activeSession?.cwd}
        onClose={() => setIsSelectServerOpen(false)}
        onSelectServer={handleConnect}
        onAddServer={handleAddServer}
        onEditServer={handleEditServer}
        onNewSession={handleNewSessionForServer}
        onCodingAgentLaunched={handleCodingAgentLaunched}
        connectedServerIds={connectedServerIds}
      />

      <ConfirmDialog
        isOpen={sessionToClose !== null}
        title={sessionToCloseObj?.purpose === 'coding_agent'
          ? t('codingAgent.close')
          : sessionToCloseObj?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
        message={sessionToCloseDirtyFileCount > 0
          ? `${sessionToCloseMessage}\n\n${t('fileWorkspace.sessionUnsavedWarning', { count: sessionToCloseDirtyFileCount })}`
          : sessionToCloseMessage}
        confirmLabel={sessionToCloseObj?.purpose === 'coding_agent'
          ? t('codingAgent.close')
          : sessionToCloseObj?.sessionType === 'local' ? t('session.closeLocalShell') : t('session.closeSession')}
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
