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
  ExternalLink,
  Rows2,
  PanelRightClose,
  Loader2,
  X,
  Bot,
  Code2,
  FileDiff,
  Blocks,
  Store,
  History,
} from 'lucide-react';
import { Mosaic, MosaicWindow, type MosaicNode, type MosaicBranch } from 'react-mosaic-component2';
import 'react-mosaic-component2/react-mosaic-component.css';
import { cn } from './lib/utils';
import { safeInvoke } from './lib/tauri';
import { useSessionStore, type Session } from './stores/sessionStore';
import { useNavigationStore } from './stores/navigationStore';
import { useNotificationStore } from './stores/notificationStore';
import { useThemeSync } from './lib/useThemeSync';
import {
  DETACHED_CLOSED_EVENT,
  removeDetachedFromLayout,
  openDetachedWindow,
  restoreDetachedWindows,
} from './lib/detach';
import { UPDATE_CHECK_INTERVAL_MS, useUpdateStore } from './stores/updateStore';
import { SessionTabs } from './components/SessionTabs';
import { TitleBar } from './components/TitleBar';
import { SftpPanel, SftpPanelHandle } from './components/SftpPanel';
import { AddServerDialog } from './components/AddServerDialog';
import { EditServerDialog } from './components/EditServerDialog';
import { ConnectDialog } from './components/ConnectDialog';
import { SelectServerDialog } from './components/SelectServerDialog';
import { QuickCommandDialog } from './components/QuickCommandDialog';
import { CommandHistoryDialog } from './components/CommandHistoryDialog';
import { ConfirmDialog } from './components/ConfirmDialog';
import { Notifications } from './components/Notifications';
import { AgentActivityPanel } from './components/AgentActivityPanel';
import { AgentApprovalDialog } from './components/AgentApprovalDialog';
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
import {
  usePluginWorkspaceStore,
  type PluginWorkspaceTab,
} from './stores/pluginWorkspaceStore';
import { useFileWorkspaceStore } from './stores/fileWorkspaceStore';
import { SessionPluginDock } from './components/SessionPluginDock';
import { PluginPanel } from './components/PluginPanel';
import { PluginTabLauncher } from './components/PluginTabLauncher';
import { PaneDropZone } from './components/PaneDropZone';
import {
  parsePaneId,
  pluginPaneId,
  sessionPaneId,
  SESSION_PANE_PREFIX,
  PLUGIN_PANE_PREFIX,
} from './lib/paneIds';
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
const PluginWorkspaceView = lazy(() => import('./components/PluginPanel/PluginWorkspaceView').then((mod) => ({ default: mod.PluginWorkspaceView })));

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
  const { checkForUpdates, markVersionNotified } = useUpdateStore();
  const servers = useServerStore((state) => state.servers);
  const fetchServers = useServerStore((state) => state.fetchServers);
  const fetchGroups = useServerStore((state) => state.fetchGroups);
  const runtimeCapabilities = useRuntimeCapabilitiesStore((state) => state.capabilities);
  const loadRuntimeCapabilities = useRuntimeCapabilitiesStore((state) => state.load);
  const isCompactWorkspace = useMediaQuery('(max-width: 767px)');
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);
  const plugins = usePluginStore((state) => state.plugins);
  const fileTabs = useFileWorkspaceStore((state) => state.tabs);
  const activeFileTabId = useFileWorkspaceStore((state) => state.activeTabId);
  const activateFileTab = useFileWorkspaceStore((state) => state.activateTab);
  const closeFileTab = useFileWorkspaceStore((state) => state.closeTab);
  const pluginTabs = usePluginWorkspaceStore((state) => state.tabs);
  const activePluginTabId = usePluginWorkspaceStore((state) => state.activeTabId);
  const activatePluginTab = usePluginWorkspaceStore((state) => state.activateTab);
  const closePluginTab = usePluginWorkspaceStore((state) => state.closeTab);

  const [isAddServerOpen, setIsAddServerOpen] = useState(false);
  const [isConnectOpen, setIsConnectOpen] = useState(false);
  const [isQuickCommandOpen, setIsQuickCommandOpen] = useState(false);
  const [isCommandHistoryOpen, setIsCommandHistoryOpen] = useState(false);
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

  useThemeSync();

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

  // Re-open the detached windows from the previous session so the workspace
  // comes back exactly as it was left.
  useEffect(() => {
    void restoreDetachedWindows();
  }, []);

  // When a torn-out tab window closes or merges back, re-activate its tab so
  // the content is immediately visible in the main window again. While the
  // whole app is quitting, the layout entry is kept so the next launch
  // restores the same set of windows.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    let appQuitting = false;
    let stopQuitListener: (() => void) | null = null;
    void import('@tauri-apps/api/event')
      .then(async ({ listen }) => {
        const { getCurrentWindow } = await import('@tauri-apps/api/window');
        // Mark quitting as soon as the main window is asked to close; detached
        // windows closing after that must not drop their layout entries.
        void getCurrentWindow()
          .listen('tauri://close-requested', () => {
            appQuitting = true;
          })
          .then((stop) => {
            if (disposed) stop();
            else stopQuitListener = stop;
          });
        return listen<{ kind: string; sessionId: string; pluginId?: string }>(
        DETACHED_CLOSED_EVENT,
        (event) => {
          const payload = event.payload;
          if (!appQuitting) {
            removeDetachedFromLayout(
              payload.kind === 'plugin' && payload.pluginId
                ? {
                    kind: 'plugin',
                    pluginId: payload.pluginId,
                    sessionId: payload.sessionId,
                    serverName: '',
                    sessionType: 'ssh',
                  }
                : { kind: 'terminal', sessionId: payload.sessionId, title: '' }
            );
          }
          if (payload.kind === 'plugin' && payload.pluginId) {
            const workspace = usePluginWorkspaceStore.getState();
            const tab = workspace.tabs.find(
              (candidate) =>
                candidate.pluginId === payload.pluginId && candidate.sessionId === payload.sessionId
            );
            if (tab) {
              useFileWorkspaceStore.getState().activateTab(null);
              workspace.activateTab(tab.id);
            }
          } else {
            usePluginWorkspaceStore.getState().activateTab(null);
            useFileWorkspaceStore.getState().activateTab(null);
            useSessionStore.getState().setActiveSession(payload.sessionId);
          }
        }
      );
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
      stopQuitListener?.();
    };
  }, []);

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

      if (isCtrl && event.key.toLowerCase() === 'w' && activePluginTabId) {
        event.preventDefault();
        closePluginTab(activePluginTabId);
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
  const activePluginTab = useMemo(
    () => pluginTabs.find((tab) => tab.id === activePluginTabId) ?? null,
    [activePluginTabId, pluginTabs]
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
    if (
      activePluginTab
      && activeSessionId !== activePluginTab.sessionId
      && sessions.some((session) => session.id === activePluginTab.sessionId)
    ) {
      setActiveSession(activePluginTab.sessionId);
    }
  }, [activePluginTab, activeSessionId, sessions, setActiveSession]);

  useEffect(() => {
    if (activeSession?.purpose !== 'coding_agent') {
      setIsWorkspaceChangesOpen(false);
    }
  }, [activeSession?.purpose]);

  // A selected tab must always reveal its terminal. Keep an existing split when
  // the active session is already one of its panes; otherwise switch to it.
  // A plugin pane stays pinned only while its tab remains active; switching to
  // another session collapses back to that session's terminal.
  useEffect(() => {
    if (!activeSessionId) return;
    setMosaicTree((current) => {
      if (current !== null && getLeaves(current).includes(sessionPaneId(activeSessionId))) {
        return current;
      }
      const pinnedPluginTabId = usePluginWorkspaceStore.getState().activeTabId;
      if (
        current !== null
        && pinnedPluginTabId !== null
        && getLeaves(current).includes(pluginPaneId(pinnedPluginTabId))
      ) {
        // The active plugin is split into the layout — keep it visible.
        return current;
      }
      return sessionPaneId(activeSessionId);
    });
  }, [activeSessionId]);

  // Prune panes whose sessions or plugin tabs have been closed, keeping the
  // layout otherwise intact.
  useEffect(() => {
    setMosaicTree((current) => {
      if (current === null) return current;
      const validIds = new Set<string>(sessions.map((session) => sessionPaneId(session.id)));
      for (const tab of pluginTabs) validIds.add(pluginPaneId(tab.id));
      const pruned = pruneLeaves(current, validIds);
      // If everything was pruned, fall back to the active session (or null).
      if (pruned === null) {
        return activeSessionId ? sessionPaneId(activeSessionId) : null;
      }
      return pruned === current ? current : pruned;
    });
  }, [sessions, pluginTabs, activeSessionId]);

  const handleConnected = useCallback((sessionId: string) => {
    console.log('[App] handleConnected called with sessionId:', sessionId);
    activateFileTab(null);
    activatePluginTab(null);
    setActiveSession(sessionId);

    // The Terminal component attaches after its event listener is ready so
    // the initial prompt/MOTD can be replayed without losing early output.
    setTimeout(() => {
      console.log('[App] Focusing terminal for session:', sessionId);
      terminalRefs.current.get(sessionId)?.focus();
    }, 100);
  }, [activateFileTab, activatePluginTab, setActiveSession]);

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
    activatePluginTab(null);
    setActiveSession(sessionId);
    setMosaicTree(sessionPaneId(sessionId));
    window.setTimeout(() => terminalRefs.current.get(sessionId)?.focus(), 100);
  }, [activateFileTab, activatePluginTab, setActiveSession]);

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

      setMosaicTree((current) => addPane(current, sessionPaneId(targetId), sessionPaneId(session.id), direction));
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

  const handleRemoveTerminalPane = useCallback((paneId: string) => {
    setMosaicTree((current) => {
      if (countLeaves(current) <= 1) return current;

      const next = removePane(current, paneId);
      // Pick a new active session from the remaining session panes.
      const remaining = getLeaves(next);
      const pane = parsePaneId(paneId);
      if (pane.kind === 'session' && activeSessionId === pane.id) {
        const nextActivePane = remaining.find((leaf) => leaf.startsWith(SESSION_PANE_PREFIX));
        if (nextActivePane) setActiveSession(nextActivePane.slice(SESSION_PANE_PREFIX.length));
      }
      return next;
    });
  }, [activeSessionId, setActiveSession]);

  const handleCollapseTerminalPanes = useCallback(() => {
    if (activeSessionId) setMosaicTree(sessionPaneId(activeSessionId));
  }, [activeSessionId]);

  // Split a plugin tab into the mosaic layout next to the active terminal.
  const handleSplitPluginTab = useCallback((tabId: string, direction: 'row' | 'column') => {
    const paneId = pluginPaneId(tabId);
    if (mosaicTree !== null && getLeaves(mosaicTree).includes(paneId)) return;
    if (mosaicTree !== null && countLeaves(mosaicTree) >= MAX_TERMINAL_PANES) {
      notifyWarning(t('session.splitLimitTitle'), t('session.splitLimitMessage'));
      return;
    }

    const leaves = mosaicTree !== null ? getLeaves(mosaicTree) : [];
    const target = activeSessionId && leaves.includes(sessionPaneId(activeSessionId))
      ? sessionPaneId(activeSessionId)
      : leaves[0] ?? null;

    setMosaicTree((current) => addPane(current, target, paneId, direction));
    // Switch back to the terminal view so the split result is visible.
    activateFileTab(null);
  }, [mosaicTree, activeSessionId, notifyWarning, t, activateFileTab]);

  // A tab dropped onto a pane edge (mouse-driven drag): plugin tabs pin a
  // plugin pane, session tabs pin another terminal pane.
  const handlePaneDropTab = useCallback((
    targetPaneId: string,
    kind: 'session' | 'plugin',
    tabId: string,
    direction: 'row' | 'column'
  ) => {
    const paneId = kind === 'plugin' ? pluginPaneId(tabId) : sessionPaneId(tabId);
    setMosaicTree((current) => {
      if (current === null || getLeaves(current).includes(paneId)) return current;
      if (countLeaves(current) >= MAX_TERMINAL_PANES) return current;
      return addPane(current, targetPaneId, paneId, direction);
    });
  }, []);

  const handleOpenPluginTab = useCallback((pluginId: string) => {
    const session = activeSession ?? sessions.find((candidate) => candidate.state === 'connected');
    if (!session) {
      notifyWarning(t('plugins.workspace'), t('plugins.openTabNeedsSession'));
      return;
    }
    usePluginWorkspaceStore.getState().openPluginTab({
      pluginId,
      sessionId: session.id,
      sessionType: session.sessionType,
      serverName: session.serverName,
    });
    activateFileTab(null);
  }, [activeSession, sessions, notifyWarning, t, activateFileTab]);

  const handleClosePluginTab = useCallback((tab: PluginWorkspaceTab) => {
    closePluginTab(tab.id);
    // If the tab was split into the layout, drop its pane as well.
    setMosaicTree((current) => (
      current === null || !getLeaves(current).includes(pluginPaneId(tab.id))
        ? current
        : removePane(current, pluginPaneId(tab.id))
    ));
  }, [closePluginTab]);

  const handleNewSessionForServer = useCallback((server: Server) => {
    handleConnect(server, { forceNew: true });
  }, [handleConnect]);

  const handleOpenSessionInWindow = useCallback((session: Session) => {
    void openDetachedWindow({
      kind: 'terminal',
      sessionId: session.id,
      title: session.serverName,
    });
  }, []);

  const handleQuickCommand = useCallback(() => {
    setIsQuickCommandOpen(true);
  }, []);

  const handleOpenCommandHistory = useCallback(() => {
    if (!activeSession || activeSession.sessionType !== 'ssh') {
      notifyWarning(t('commandHistory.title'), t('commandHistory.connectFirst'));
      return;
    }
    setIsCommandHistoryOpen(true);
  }, [activeSession, notifyWarning, t]);

  const handleUseHistoryCommand = useCallback((command: string) => {
    if (!activeSessionId || activeSession?.sessionType !== 'ssh') return;
    terminalRefs.current.get(activeSessionId)?.sendCommand(command);
  }, [activeSession?.sessionType, activeSessionId]);

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
  const pluginPaneLeaves = useMemo(
    () => new Set(getLeaves(mosaicTree).filter((leaf) => leaf.startsWith(PLUGIN_PANE_PREFIX))),
    [mosaicTree]
  );
  const activePluginTabPinned = activePluginTab !== null
    && pluginPaneLeaves.has(pluginPaneId(activePluginTab.id));
  // The terminal mosaic is hidden behind file tabs and behind an unpinned
  // plugin tab (a pinned plugin already lives inside the mosaic itself).
  const terminalAreaHidden = activeFileTab !== null
    || (activePluginTab !== null && !activePluginTabPinned);

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
              onOpenSessionInWindow={handleOpenSessionInWindow}
              onPaneDropTab={handlePaneDropTab}
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
                        id: 'command-history',
                        label: t('sidebar.history'),
                        icon: <History className="h-4 w-4" />,
                        disabled: !activeSession || activeSession.sessionType !== 'ssh',
                        pressed: isCommandHistoryOpen,
                        onSelect: handleOpenCommandHistory,
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
                    className={cn('icon-button tooltip-button', isCommandHistoryOpen && 'is-active')}
                    data-tooltip={t('sidebar.history')}
                    onClick={handleOpenCommandHistory}
                    disabled={!activeSession || activeSession.sessionType !== 'ssh'}
                    aria-pressed={isCommandHistoryOpen}
                    aria-label={t('sidebar.history')}
                  >
                    <History className="h-4 w-4" />
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
                  {activeSession && (
                    <button
                      className="icon-button tooltip-button"
                      data-tooltip={t('session.openInWindow')}
                      aria-label={t('session.openInWindow')}
                      onClick={() => handleOpenSessionInWindow(activeSession)}
                    >
                      <ExternalLink className="h-4 w-4" />
                    </button>
                  )}
                  <PluginTabLauncher
                    sessionType={activeSession ? activeSession.sessionType : null}
                    onOpenPluginTab={handleOpenPluginTab}
                  />
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
                      {
                        id: 'command-history',
                        label: t('sidebar.history'),
                        icon: <History className="h-4 w-4" />,
                        disabled: !activeSession || activeSession.sessionType !== 'ssh',
                        pressed: isCommandHistoryOpen,
                        onSelect: handleOpenCommandHistory,
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
                {pluginTabs.map((tab) => (
                  <div
                    key={tab.id}
                    className={cn(
                      'min-h-0 flex-1',
                      activePluginTabId === tab.id && activeFileTab === null && !pluginPaneLeaves.has(pluginPaneId(tab.id))
                        ? 'block'
                        : 'hidden'
                    )}
                  >
                    <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                      <PluginWorkspaceView
                        tab={tab}
                        plugin={plugins.find((candidate) => candidate.manifest.id === tab.pluginId)}
                        onClose={() => handleClosePluginTab(tab)}
                        onSplit={handleSplitPluginTab}
                      />
                    </Suspense>
                  </div>
                ))}
                <div className={cn('min-h-0 flex-1 flex-col', terminalAreaHidden ? 'hidden' : 'flex')}>
                  {sessions.length > 0 ? (
                    <>
                    <div className="mosaic-container relative min-h-0 flex-1 p-2">
                      <Mosaic<string>
                        value={mosaicTree}
                        onChange={(node) => setMosaicTree(node)}
                        renderTile={(id: string, path: MosaicBranch[]) => {
                          const pane = parsePaneId(id);

                          if (pane.kind === 'plugin') {
                            const tab = pluginTabs.find((candidate) => candidate.id === pane.id);
                            const plugin = tab
                              ? plugins.find((candidate) => candidate.manifest.id === tab.pluginId)
                              : undefined;
                            const pluginName = plugin
                              ? t(`plugins.catalog.${plugin.manifest.id}.name`, { defaultValue: plugin.manifest.name })
                              : tab?.pluginId ?? id;
                            const paneTitle = tab ? `${pluginName} · ${tab.serverName}` : pluginName;
                            return (
                              <MosaicWindow<string>
                                path={path}
                                title={paneTitle}
                                draggable
                                toolbarControls={canRemoveTerminalPane ? [
                                  <button
                                    key="close"
                                    className="icon-button h-5 w-5"
                                    onClick={() => handleRemoveTerminalPane(id)}
                                    aria-label={t('session.removePane', { name: paneTitle })}
                                    title={t('session.removePane', { name: paneTitle })}
                                  >
                                    <X className="h-3 w-3" />
                                  </button>,
                                ] : []}
                              >
                                <PaneDropZone paneId={id}>
                                  {tab && plugin ? (
                                    <PluginPanel
                                      stateKey={tab.id}
                                      plugin={plugin}
                                      sessionId={tab.sessionId}
                                      sessionType={tab.sessionType}
                                      variant="workspace"
                                    />
                                  ) : (
                                    <div className="flex h-full items-center justify-center text-xs text-tokyo-comment">
                                      {t('plugins.tabPluginMissing')}
                                    </div>
                                  )}
                                </PaneDropZone>
                              </MosaicWindow>
                            );
                          }

                          const session = sessions.find((s) => s.id === pane.id);
                          const serverName = session?.serverName ?? t('session.localShell');
                          return (
                            <MosaicWindow<string>
                              path={path}
                              title={serverName}
                              draggable
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
                              className={cn(activeSessionId && id === sessionPaneId(activeSessionId) && 'mosaic-window-active')}
                            >
                              <PaneDropZone paneId={id}>
                                <div
                                  className="relative h-full bg-tokyo-bg"
                                  onMouseDown={() => {
                                    if (pane.id !== activeSessionId) setActiveSession(pane.id);
                                  }}
                                >
                                  <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                                    <Terminal
                                      ref={(handle: TerminalHandle | null) => {
                                        if (handle) {
                                          terminalRefs.current.set(pane.id, handle);
                                        } else {
                                          terminalRefs.current.delete(pane.id);
                                        }
                                      }}
                                      sessionId={pane.id}
                                      onData={handleData}
                                    />
                                  </Suspense>
                                </div>
                              </PaneDropZone>
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
                        onOpenPluginTab={handleOpenPluginTab}
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

      <CommandHistoryDialog
        isOpen={isCommandHistoryOpen}
        serverId={activeSession?.sessionType === 'ssh' ? activeSession.serverId : undefined}
        serverName={activeSession?.sessionType === 'ssh' ? activeSession.serverName : undefined}
        onClose={() => setIsCommandHistoryOpen(false)}
        onUseCommand={handleUseHistoryCommand}
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

      {runtimeCapabilities.agentGateway && <AgentApprovalDialog />}
    </div>
  );
}

export default App;
