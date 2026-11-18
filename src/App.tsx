import { useCallback, useState, useRef, useEffect, useMemo, lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import {
  FolderOpen,
  Settings as SettingsIcon,
  ArrowLeft,
  Terminal as TerminalIcon,
  ArrowRightLeft,
  Activity,
  Loader2,
  ExternalLink,
  X,
  FileDiff,
  Blocks,
  Store,
  History,
} from 'lucide-react';
import { Mosaic, MosaicWindow, type MosaicNode, type MosaicBranch } from 'react-mosaic-component2';
import 'react-mosaic-component2/react-mosaic-component.css';
import { cn } from './lib/utils';
import { safeInvoke } from './lib/tauri';
import { withDeadline } from './lib/deadline';
import { useSessionStore, type Session } from './stores/sessionStore';
import { useNavigationStore } from './stores/navigationStore';
import { useNotificationStore } from './stores/notificationStore';
import { useThemeSync } from './lib/useThemeSync';
import { listenForLocalFileRequests, openLocalFiles } from './lib/localFiles';
import {
  openDetachedWindow, useDetachedOwnership, detachTargetKey, hydrateTransfer, canCloseWorkspaceSession,
  releaseDetached, readDetachedLayout, WORKSPACE_QUITTING_EVENT, type TransferSnapshot,
} from './lib/detach';
import { dockPane, replaceLeaf, type DockSide, type PanePlacement } from './lib/docking';
import { receiveWindowDrops } from './lib/nativeDock';
import { trackWindowGeometry, captureWindowGeometry } from './lib/windowGeometry';
import { restoreWorkspaceLayout, restoreWorkspaceWindows, saveWorkspaceLayout } from './lib/workspacePersistence';
import { UPDATE_CHECK_INTERVAL_MS, useUpdateStore } from './stores/updateStore';
import { SessionTabs } from './components/SessionTabs';
import { TitleBar } from './components/TitleBar';
import { SftpPanel, SftpPanelHandle } from './components/SftpPanel';
import { AddServerDialog } from './components/AddServerDialog';
import { EditServerDialog } from './components/EditServerDialog';
import { ConnectDialog } from './components/ConnectDialog';
import { SelectServerDialog } from './components/SelectServerDialog';
import { ConfirmDialog } from './components/ConfirmDialog';
import { Notifications } from './components/Notifications';
import { AgentActivityPanel } from './components/AgentActivityPanel';
import { AgentApprovalDialog } from './components/AgentApprovalDialog';
import { WorkspaceChangesPanel } from './components/WorkspaceChangesPanel';
import { isPluginCompatible, localizedPluginName } from './plugins/pluginUtils';
import { WorkspaceToolbar } from './components/WorkspaceToolbar';
import { ContainerPanel } from './components/ContainerPanel';
import { FingerprintVerificationDialog, FingerprintManagerDialog } from './components/FingerprintDialog';
import { SnippetManagerDialog } from './components/SnippetManager/SnippetManagerDialog';
import { TunnelPanelDialog } from './components/TunnelPanel/TunnelPanelDialog';
import { useServerStore, type Server, isTeleportServer } from './stores/serverStore';
import { useRuntimeCapabilitiesStore } from './stores/runtimeCapabilitiesStore';
import { useMediaQuery } from './lib/useMediaQuery';
import { usePluginStore } from './stores/pluginStore';
import {
  usePluginWorkspaceStore,
  type PluginWorkspaceTab,
} from './stores/pluginWorkspaceStore';
import { useFileWorkspaceStore } from './stores/fileWorkspaceStore';
import { SessionPluginDock } from './components/SessionPluginDock';
import { PerformanceDock } from './components/ServerStatus/PerformanceDock';
import { PluginPanel } from './components/PluginPanel';
import { PaneDropZone } from './components/PaneDropZone';
import {
  parsePaneId,
  filePaneId,
  pluginPaneId,
  sessionPaneId,
  SESSION_PANE_PREFIX,
  PLUGIN_PANE_PREFIX,
} from './lib/paneIds';
import type { TerminalHandle } from './components/Terminal';
import {
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

function activateWorkspacePane(id: string): void {
  const pane = parsePaneId(id);
  const files = useFileWorkspaceStore.getState();
  const plugins = usePluginWorkspaceStore.getState();
  files.activateTab(pane.kind === 'file' ? pane.id : null);
  plugins.activateTab(pane.kind === 'plugin' ? pane.id : null);
  const sessionId = pane.kind === 'session' ? pane.id
    : pane.kind === 'file' ? files.tabs.find((tab) => tab.id === pane.id)?.sessionId
    : plugins.tabs.find((tab) => tab.id === pane.id)?.sessionId;
  if (sessionId && useSessionStore.getState().sessions.some(session => session.id === sessionId)) {
    useSessionStore.getState().setActiveSession(sessionId);
  }
}

function App() {
  const { t } = useTranslation();
  const {
    sessions,
    activeSessionId,
    setActiveSession,
    killSession,
    killLocalShellSession,
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
  const performanceBusy = usePluginStore((state) => state.operationId === 'server-performance');
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
  const [isEditServerOpen, setIsEditServerOpen] = useState(false);
  const [isSelectServerOpen, setIsSelectServerOpen] = useState(false);
  const [isSnippetManagerOpen, setIsSnippetManagerOpen] = useState(false);
  const [isTunnelPanelOpen, setIsTunnelPanelOpen] = useState(false);
  const [isSftpOpen, setIsSftpOpen] = useState(false);
  const [containerParentId, setContainerParentId] = useState<string | null>(null);
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [isWorkspaceChangesOpen, setIsWorkspaceChangesOpen] = useState(false);
  const [isPluginDockOpen, setIsPluginDockOpen] = useState(false);
  const [isPerformanceOpen, setIsPerformanceOpen] = useState(false);
  const [serverToConnect, setServerToConnect] = useState<Server | null>(null);
  const [connectForceNew, setConnectForceNew] = useState(false);
  const [serverToEdit, setServerToEdit] = useState<Server | null>(null);
  const [sessionToClose, setSessionToClose] = useState<string | null>(null);
  const [mosaicTree, setMosaicTree] = useState<MosaicNode<string> | null>(null);
  const [workspaceReady, setWorkspaceReady] = useState(!('__TAURI_INTERNALS__' in window));
  const workspaceInitialized = useRef(!('__TAURI_INTERNALS__' in window));
  const detachedOwners = useDetachedOwnership((state) => state.owners);
  const treeRef = useRef(mosaicTree);
  treeRef.current = mosaicTree;
  const focusedPaneRef = useRef<string | null>(null);
  const dockReceiverRef = useRef<(snapshot: TransferSnapshot, placement: PanePlacement | null) => boolean>(() => false);

  // Per-pane terminal handles. Mosaic can render multiple panes at once, so a
  // single ref is insufficient; each pane registers/unregisters via callback ref.
  const terminalRefs = useRef<Map<string, TerminalHandle>>(new Map());
  const sftpPanelRef = useRef<SftpPanelHandle>(null);
  const sessionBootstrapRef = useRef<Promise<void> | null>(null);

  useThemeSync();

  useEffect(() => {
    if (!workspaceReady) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    // Restoring layout must finish before files received from Finder are added.
    void Promise.resolve(sessionBootstrapRef.current).then(() => {
      if (disposed) return;
      return listenForLocalFileRequests().then(unlisten => { if (disposed) unlisten(); else stop = unlisten; });
    }).catch(console.error);
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'o') {
        event.preventDefault(); if (!event.repeat) void openLocalFiles();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => { disposed = true; stop?.(); window.removeEventListener('keydown', onKey); };
  }, [workspaceReady]);

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
        try {
          const capabilities = await loadRuntimeCapabilities();
          await fetchSessions();
          if ('__TAURI_INTERNALS__' in window) {
            await syncRemoteSessions();
            const restored = await restoreWorkspaceLayout(capabilities.localShell);
            if (restored.layout) {
              focusedPaneRef.current = restored.layout.focusedPane;
              treeRef.current = restored.layout.tree;
              setMosaicTree(restored.layout.tree);
            }
            setWorkspaceReady(true);
            await restoreWorkspaceWindows(restored.layout);
            if (restored.warnings.length) {
              useNotificationStore.getState().warning(
                t('workspaceLayout.restored'),
                t('workspaceLayout.restoreAttention', { names: restored.warnings.join(', ') })
              );
            }
          }
          if (capabilities.localShell && useSessionStore.getState().sessions.length === 0) {
            await createLocalShellSession(undefined, 80, 24);
          }
          workspaceInitialized.current = true;
        } catch (error) {
          notifyError(t('workspaceLayout.restoreFailed'), String(error));
        } finally { setWorkspaceReady(true); }
      })();
    }

    // Poll session state every 2s, but pause while the window is hidden to
    // avoid pointless IPC when the app is in the background. On regaining
    // visibility, sync immediately so stale UI refreshes without waiting.
    let intervalId: number | null = window.setInterval(() => {
      if (document.hidden || !workspaceInitialized.current) return;
      void syncRemoteSessions();
    }, 2000);

    const handleVisibilityChange = () => {
      if (document.hidden) {
        if (intervalId !== null) {
          window.clearInterval(intervalId);
          intervalId = null;
        }
      } else {
        if (workspaceInitialized.current) void syncRemoteSessions();
        if (intervalId === null) {
          intervalId = window.setInterval(() => {
            if (document.hidden || !workspaceInitialized.current) return;
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
    if (!('__TAURI_INTERNALS__' in window)) return;
    let disposed = false;
    let closing = false;
    const stops: (() => void)[] = [];
    const keep = (stop: () => void) => disposed ? stop() : stops.push(stop);
    const flush = () => {
      if (!workspaceInitialized.current) return;
      saveWorkspaceLayout(treeRef.current, focusedPaneRef.current);
    };
    const quit = async () => {
      if (closing) return;
      closing = true;
      try {
        try { flush(); } catch (error) {
          notifyError(t('workspaceLayout.saveFailed'), String(error));
          if (!window.confirm(t('workspaceLayout.quitWithoutLayout'))) return;
        }
        try {
          await withDeadline((async () => {
            const { getCurrentWindow, Window: NativeWindow } = await import('@tauri-apps/api/window');
            await captureWindowGeometry('main', getCurrentWindow());
            for (const entry of readDetachedLayout()) {
              const label = useDetachedOwnership.getState().owners[detachTargetKey(entry.target)];
              const native = label ? await NativeWindow.getByLabel(label) : null;
              if (native) await captureWindowGeometry(entry.geometryKey, native);
            }
          })(), 1500, 'Window geometry capture timed out');
        } catch (error) { console.warn('[Workspace] Keeping last saved window bounds:', error); }
        const { emit } = await import('@tauri-apps/api/event');
        await withDeadline(emit(WORKSPACE_QUITTING_EVENT, { quitting: true }), 1000, 'Quit notification timed out').catch(console.warn);
        const result = await safeInvoke('workspace_exit');
        if (!result.success) throw result.error;
      } catch (error) {
        const { emit } = await import('@tauri-apps/api/event');
        void emit(WORKSPACE_QUITTING_EVENT, { quitting: false }).catch(console.warn);
        useNotificationStore.getState().error(t('workspaceLayout.saveFailed'), String(error));
      } finally { closing = false; }
    };
    void receiveWindowDrops((snapshot, placement) => dockReceiverRef.current(snapshot, placement)).then(keep).catch(console.error);
    void trackWindowGeometry('main').then(keep).catch(console.error);
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().onCloseRequested((event) => {
      event.preventDefault(); void quit();
    })).then(keep).catch(console.error);
    void import('@tauri-apps/api/event').then(({ listen }) => listen('vibeshell://save-and-quit', () => { void quit(); })).then((stop) => {
      keep(stop);
      if (!disposed) return safeInvoke('workspace_save_handler_ready');
    }).catch(console.error);
    const flushOnHide = () => { try { flush(); } catch (error) { console.error('[Workspace] Save failed:', error); } };
    window.addEventListener('pagehide', flushOnHide);
    return () => { disposed = true; stops.forEach((stop) => stop()); window.removeEventListener('pagehide', flushOnHide); };
  }, [t]);

  useEffect(() => {
    if (!workspaceReady || !workspaceInitialized.current || !('__TAURI_INTERNALS__' in window)) return;
    const timer = setTimeout(() => {
      try { saveWorkspaceLayout(treeRef.current, focusedPaneRef.current); }
      catch (error) { notifyError(t('workspaceLayout.saveFailed'), String(error)); }
    }, 200);
    return () => clearTimeout(timer);
  }, [workspaceReady, mosaicTree, sessions, fileTabs, pluginTabs, detachedOwners, activeSessionId, activeFileTabId, activePluginTabId, notifyError, t]);

  const closeInactiveSession = useCallback(async (session: Session) => {
    if (!canCloseWorkspaceSession(session.id)) return false;
    // killSession/killLocalShellSession remove the tab on success (and when the
    // backend reports the session already gone). On other failures the tab is
    // kept so the periodic sync can reconcile instead of the tab flickering
    // back after a forced removal.
    if (session.sessionType === 'local') {
      await killLocalShellSession(session.id);
    } else {
      await killSession(session.id);
    }
    return true;
  }, [killSession, killLocalShellSession]);

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

  // Selecting a new page in a split replaces only the focused pane, never the entire layout.
  useEffect(() => {
    if (!workspaceReady) return;
    const selected = activeFileTabId ? filePaneId(activeFileTabId)
      : activePluginTabId ? pluginPaneId(activePluginTabId)
      : activeSessionId ? sessionPaneId(activeSessionId) : null;
    if (!selected || detachedOwners[selected]) return;
    setMosaicTree((current) => {
      const leaves = getLeaves(current);
      if (leaves.includes(selected)) { focusedPaneRef.current = selected; return current; }
      if (leaves.length > 1) {
        const target = focusedPaneRef.current && leaves.includes(focusedPaneRef.current) ? focusedPaneRef.current : leaves[0];
        focusedPaneRef.current = selected;
        return replaceLeaf(current, target, selected);
      }
      // Standalone file/plugin pages keep their previous terminal alive until explicitly split.
      return parsePaneId(selected).kind === 'session' ? selected : current;
    });
  }, [workspaceReady, activeSessionId, activeFileTabId, activePluginTabId, detachedOwners]);

  useEffect(() => {
    if (!workspaceReady) return;
    const valid = new Set(sessions.map((session) => sessionPaneId(session.id)));
    for (const tab of pluginTabs) valid.add(pluginPaneId(tab.id));
    for (const tab of fileTabs) valid.add(filePaneId(tab.id));
    for (const id of Object.keys(detachedOwners)) valid.delete(id);
    const fallback = activeSessionId && valid.has(sessionPaneId(activeSessionId))
      ? sessionPaneId(activeSessionId) : [...valid][0] ?? null;
    setMosaicTree((current) => pruneLeaves(current, valid) ?? fallback);
    const selected = activeFileTabId ? filePaneId(activeFileTabId)
      : activePluginTabId ? pluginPaneId(activePluginTabId)
      : activeSessionId ? sessionPaneId(activeSessionId) : null;
    if (selected && detachedOwners[selected]) {
      const next = getLeaves(treeRef.current).find((id) => valid.has(id)) ?? fallback;
      if (next) activateWorkspacePane(next);
      else { activateFileTab(null); activatePluginTab(null); setActiveSession(null); }
    }
  }, [workspaceReady, sessions, pluginTabs, fileTabs, detachedOwners, activeSessionId, activeFileTabId, activePluginTabId, activateFileTab, activatePluginTab, setActiveSession]);

  const handleConnected = useCallback((sessionId: string) => {
    activateFileTab(null);
    activatePluginTab(null);
    setActiveSession(sessionId);

    // The Terminal component attaches after its event listener is ready so
    // the initial prompt/MOTD can be replayed without losing early output.
    setTimeout(() => {
      terminalRefs.current.get(sessionId)?.focus();
    }, 100);
  }, [activateFileTab, activatePluginTab, setActiveSession]);

  const handleConnect = useCallback(async (server: Server, options?: { forceNew?: boolean }) => {
    const forceNew = options?.forceNew ?? false;

    if (isTeleportServer(server)) {
      const session = await connectWithCredentials(
        server.name,
        'password',
        '',
        undefined,
        80,
        24,
        forceNew
      );
      if (session) {
        handleConnected(session.id);
      }
      return;
    }

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
        handleConnected(session.id);
      } else {
        setServerToConnect(server);
        setConnectForceNew(forceNew);
        setIsConnectOpen(true);
      }
    } else {
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

    if (session.container) {
      const parent = useSessionStore.getState().getSession(session.container.parent_session_id);
      if (parent?.state === 'connected') setContainerParentId(parent.id);
      else notifyWarning('Container parent disconnected', 'Reconnect the host session, then select the container again. This container tab will never reconnect as a host shell.');
      return;
    }

    const server = await resolveServerForSession(session);
    if (!server) {
      notifyWarning('Server Not Found', `Could not find a saved server for ${session.serverName}.`);
      return;
    }

    if (!await closeInactiveSession(session)) return;
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


  const handleCodingAgentLaunched = useCallback((sessionId: string) => {
    activateFileTab(null);
    activatePluginTab(null);
    setActiveSession(sessionId);
    setMosaicTree(sessionPaneId(sessionId));
    window.setTimeout(() => terminalRefs.current.get(sessionId)?.focus(), 100);
  }, [activateFileTab, activatePluginTab, setActiveSession]);

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

  const handlePaneDropTab = useCallback((targetPaneId: string, kind: 'session' | 'plugin' | 'file', tabId: string, direction: 'row' | 'column', side?: DockSide) => {
    const paneId = kind === 'file' ? filePaneId(tabId) : kind === 'plugin' ? pluginPaneId(tabId) : sessionPaneId(tabId);
    const base = getLeaves(treeRef.current).includes(targetPaneId) ? treeRef.current : targetPaneId;
    const next = dockPane(base, targetPaneId, paneId, side ?? (direction === 'row' ? 'right' : 'bottom'));
    if (next === base && targetPaneId !== paneId) {
      notifyWarning(t('session.splitLimitTitle'), t('session.splitLimitMessage'));
      return;
    }
    treeRef.current = next;
    focusedPaneRef.current = paneId;
    setMosaicTree(next);
    activateWorkspacePane(paneId);
  }, [notifyWarning, t]);

  dockReceiverRef.current = (snapshot, placement) => {
    const id = detachTargetKey(snapshot.target);
    const current = treeRef.current;
    const base = placement && !getLeaves(current).includes(placement.paneId) ? placement.paneId : current;
    const next = placement ? dockPane(base, placement.paneId, id, placement.side)
      : getLeaves(current).includes(id) ? current
      : countLeaves(current) > 1 ? replaceLeaf(current, focusedPaneRef.current && getLeaves(current).includes(focusedPaneRef.current) ? focusedPaneRef.current : getLeaves(current)[0], id)
      : id;
    if (placement && next === base && !getLeaves(base).includes(id)) {
      notifyWarning(t('session.splitLimitTitle'), t('session.splitLimitMessage'));
      return false;
    }
    hydrateTransfer(snapshot);
    releaseDetached(snapshot.target);
    treeRef.current = next;
    focusedPaneRef.current = id;
    setMosaicTree(next);
    activateWorkspacePane(id);
    try { saveWorkspaceLayout(next, id); } catch (error) { notifyError(t('workspaceLayout.saveFailed'), String(error)); }
    return true;
  };

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

  // A first-use click enables the bundled, read-only performance view. Nothing
  // is installed in the background and no external plugin is downloaded.
  const handleOpenPerformance = useCallback(async () => {
    if (!activeSession || activeSession.state !== 'connected') return;
    if (isPerformanceOpen && activePluginTabId === null && activeFileTabId === null) {
      setIsPerformanceOpen(false);
      return;
    }
    const store = usePluginStore.getState();
    if (store.operationId === 'server-performance') return;
    let plugin = store.plugins.find((item) => item.manifest.id === 'server-performance');
    if (!plugin || plugin.source !== 'builtin') {
      notifyWarning(t('sidebar.performance'), t('plugins.tabPluginMissing'));
      return;
    }
    if (!plugin.installed && !await store.installPlugin('server-performance')) {
      notifyError(t('sidebar.performance'), usePluginStore.getState().error ?? t('plugins.tabPluginMissing'));
      return;
    }
    plugin = usePluginStore.getState().plugins.find((item) => item.manifest.id === 'server-performance');
    if (!plugin?.enabled && !await store.setPluginEnabled('server-performance', true)) {
      notifyError(t('sidebar.performance'), usePluginStore.getState().error ?? t('plugins.tabPluginMissing'));
      return;
    }
    // Installation can finish after the user switches or closes a session.
    if (useSessionStore.getState().activeSessionId !== activeSession.id) return;
    activateFileTab(null);
    activatePluginTab(null);
    setIsPluginDockOpen(false);
    setIsPerformanceOpen(true);
  }, [activeSession, isPerformanceOpen, activePluginTabId, activeFileTabId, activateFileTab, activatePluginTab, notifyError, notifyWarning, t]);

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
      setIsHistoryOpen(false);
      setIsWorkspaceChangesOpen(false);
    }
    sftpPanelRef.current?.toggle();
  }, [activeSession, isSftpOpen, notifyWarning]);

  const handleOpenHistory = useCallback(() => {
    setIsHistoryOpen((current) => {
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
        setIsHistoryOpen(false);
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
    if (!canCloseWorkspaceSession(sessionId)) { setSessionToClose(null); return; }
    const session = sessions.find((s) => s.id === sessionId);
    setSessionToClose(null);

    // The store removes the tab on success (and when the backend reports the
    // session already gone); on other failures the tab stays so the sync poll
    // can reconcile instead of the tab being resurrected after force-removal.
    if (session?.sessionType === 'local') {
      await killLocalShellSession(sessionId);
    } else {
      await killSession(sessionId);
    }
  }, [sessionToClose, sessions, killSession, killLocalShellSession]);

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
  const filePaneLeaves = new Set(getLeaves(mosaicTree).filter((leaf) => parsePaneId(leaf).kind === 'file'));
  const terminalAreaHidden = (activeFileTab !== null && !filePaneLeaves.has(filePaneId(activeFileTab.id)) && !detachedOwners[filePaneId(activeFileTab.id)])
    || (activePluginTab !== null && !activePluginTabPinned && !detachedOwners[pluginPaneId(activePluginTab.id)]);

  return (
    <div className="app-shell h-screen flex flex-col">
      <div
        className="workspace-overlay glass-workspace absolute inset-0 z-30 flex flex-col"
        style={{ display: isOverlayView ? 'flex' : 'none' }}
      >
        <TitleBar activeSessionName={isPluginsView ? t('plugins.marketplace') : t('settings.title')} />
        <Notifications />
        <header className="glass-toolbar h-11 flex items-center px-4 border-b border-tokyo-bg-hl">
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
        </header>
        <div className="flex-1 overflow-y-auto">
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
        className="min-h-0 h-full flex flex-col flex-1"
        style={{ display: isOverlayView ? 'none' : 'flex' }}
      >
        <Notifications />

        <div className="min-h-0 flex-1 flex overflow-hidden">
          <main className="main-workspace flex-1 flex flex-col min-w-0 relative overflow-x-hidden w-full max-w-full">
            <TitleBar>
            <SessionTabs
              onNewSession={handleNewSession}
              onReconnectSession={handleReconnectSession}
              onOpenSessionInWindow={handleOpenSessionInWindow}
              onPaneDropTab={handlePaneDropTab}
              rightActions={(
                  <div className="titlebar-actions flex flex-shrink-0 items-center gap-1">
                  <button
                    type="button"
                    className={cn('glass-action tooltip-button', isSftpOpen && 'is-active')}
                    data-tooltip={t('sidebar.sftp')}
                    aria-label={t('sidebar.sftp')}
                    aria-pressed={isSftpOpen}
                    disabled={!activeSession || activeSession.state !== 'connected' || !!activeSession.container}
                    onClick={handleOpenSftp}
                  >
                    <FolderOpen className="h-4 w-4" aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className={cn('glass-action tooltip-button', isPerformanceOpen && !terminalAreaHidden && 'is-active')}
                    data-tooltip={t('sidebar.performance')}
                    aria-label={t('sidebar.performance')}
                    aria-pressed={isPerformanceOpen && !terminalAreaHidden}
                    aria-busy={performanceBusy}
                    disabled={!activeSession || activeSession.state !== 'connected' || performanceBusy}
                    onClick={() => { void handleOpenPerformance(); }}
                  >
                    {performanceBusy ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" /> : <Activity className="h-4 w-4" aria-hidden="true" />}
                  </button>
                  <button
                    className={cn('icon-button tooltip-button', (runtimeCapabilities.isMobile || isCompactWorkspace) && 'h-11 w-11', isHistoryOpen && 'is-active')}
                    data-tooltip={t('sidebar.history')}
                    onClick={handleOpenHistory}
                    aria-pressed={isHistoryOpen}
                    aria-label={t('sidebar.history')}
                  >
                    <History className="h-4 w-4" aria-hidden="true" />
                  </button>
                  <WorkspaceToolbar
                    label={t('common.more')}
                    anyPressed={isWorkspaceChangesOpen || isPluginDockOpen}
                    items={[
                      { id: 'containers', label: 'Docker containers', icon: <Blocks className="h-4 w-4" />, disabled: !activeSession || activeSession.sessionType !== 'ssh' || activeSession.state !== 'connected' || !!activeSession.container, onSelect: () => { if (activeSession) setContainerParentId(activeSession.id); } },
                      ...(activeSession ? plugins.filter((p) => p.manifest.id !== 'server-performance' && isPluginCompatible(p, activeSession.sessionType)).map((p) => ({
                        id: `plugin-${p.manifest.id}`, label: localizedPluginName(t, p), icon: <Blocks className="h-4 w-4" />, onSelect: () => handleOpenPluginTab(p.manifest.id),
                      })) : []),
                      ...(runtimeCapabilities.backgroundTunnels ? [{
                        id: 'tunnels',
                        label: t('sidebar.tunnels'),
                        disabled: !!activeSession?.container,
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
                        id: 'workspace-changes',
                        label: t('workspaceChanges.title'),
                        icon: <FileDiff className="h-4 w-4" />,
                        disabled: activeSession?.purpose !== 'coding_agent' || !activeSession.cwd,
                        pressed: isWorkspaceChangesOpen,
                        onSelect: handleOpenWorkspaceChanges,
                      },
                      {
                        id: 'plugin-workspace',
                        label: t('plugins.workspace'),
                        icon: <Blocks className="h-4 w-4" />,
                        disabled: !activeSession,
                        pressed: isPluginDockOpen,
                        onSelect: () => { setIsPerformanceOpen(false); setIsPluginDockOpen((open) => !open); },
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
                  />
                  </div>
              )}
            />
            </TitleBar>

            <div className="relative flex min-h-0 flex-1">
              <div className="workspace-return-zone flex min-w-0 flex-1 flex-col" onMouseDownCapture={(event) => {
                const pane = (event.target as Element).closest<HTMLElement>('[data-pane-id]');
                if (pane?.dataset.paneId) {
                  focusedPaneRef.current = pane.dataset.paneId;
                  activateWorkspacePane(pane.dataset.paneId);
                }
              }}>
                {fileTabs.filter((tab) => !filePaneLeaves.has(filePaneId(tab.id)) && !detachedOwners[filePaneId(tab.id)]).map((tab) => (
                  <div key={tab.id} data-pane-id={filePaneId(tab.id)} className={cn('relative min-h-0 flex-1', activeFileTabId === tab.id ? 'block' : 'hidden')}>
                    <Suspense fallback={<div className="h-full bg-tokyo-bg" />}>
                      <FileWorkspace tab={tab} isActive={activeFileTabId === tab.id} />
                    </Suspense>
                  </div>
                ))}
                {pluginTabs.filter((tab) => !pluginPaneLeaves.has(pluginPaneId(tab.id)) && !detachedOwners[pluginPaneId(tab.id)]).map((tab) => (
                  <div
                    data-pane-id={pluginPaneId(tab.id)}
                    key={tab.id}
                    className={cn(
                      'relative min-h-0 flex-1',
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
                      />
                    </Suspense>
                  </div>
                ))}
                <div className={cn('min-h-0 flex-1 flex-col', terminalAreaHidden ? 'hidden' : 'flex')}>
                  {sessions.length > 0 || mosaicTree !== null ? (
                    <>
                    <div className={cn('mosaic-container relative min-h-0 flex-1', !canRemoveTerminalPane && 'mosaic-single')}>
                      <Mosaic<string>
                        className="vibeshell-mosaic"
                        value={mosaicTree}
                        onChange={(node) => { treeRef.current = node; setMosaicTree(node); }}
                        zeroStateView={<div className="workspace-return-zone flex h-full items-center justify-center p-8 text-center text-sm text-tokyo-comment">{t(workspaceReady ? 'workspaceLayout.empty' : 'common.loading')}</div>}
                        renderTile={(id: string, path: MosaicBranch[]) => {
                          const pane = parsePaneId(id);
                          if (detachedOwners[id] || !workspaceReady) return <div className="h-full bg-tokyo-bg" />;
                          if (pane.kind === 'file') {
                            const tab = fileTabs.find((candidate) => candidate.id === pane.id);
                            if (!tab) return <div />;
                            return (
                              <MosaicWindow<string> path={path} title={`${tab.dirty ? '● ' : ''}${tab.name}`} draggable
                                className={cn('vibe-mosaic-pane--file', activeFileTabId === tab.id && 'mosaic-window-active')}
                                toolbarControls={[
                                  <button key="detach" type="button" className="pane-chrome-button pane-chrome-detach" aria-label={t('workspaceLayout.detach')} title={t('workspaceLayout.detach')} onClick={() => { void openDetachedWindow({ kind: 'file', sessionId: tab.sessionId, path: tab.path, name: tab.name, size: tab.size }); }}><ExternalLink className="h-3.5 w-3.5" aria-hidden="true" /></button>,
                                  ...(canRemoveTerminalPane ? [<button key="close" type="button" className="pane-chrome-button pane-chrome-close" onClick={() => handleRemoveTerminalPane(id)} aria-label={t('session.removePane', { name: tab.name })} title={t('session.removePane', { name: tab.name })}><X className="h-3.5 w-3.5" aria-hidden="true" /></button>] : []),
                                ]}>
                                <PaneDropZone paneId={id}><Suspense fallback={<div className="h-full bg-tokyo-bg" />}><FileWorkspace tab={tab} isActive={activeFileTabId === tab.id} /></Suspense></PaneDropZone>
                              </MosaicWindow>
                            );
                          }

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
                                className={cn('vibe-mosaic-pane--plugin', activePluginTabId === tab?.id && 'mosaic-window-active')}
                                toolbarControls={canRemoveTerminalPane ? [
                                  <button
                                    key="close"
                                    type="button"
                                    className="pane-chrome-button pane-chrome-close"
                                    onClick={() => handleRemoveTerminalPane(id)}
                                    aria-label={t('session.removePane', { name: paneTitle })}
                                    title={t('session.removePane', { name: paneTitle })}
                                  >
                                    <X className="h-3.5 w-3.5" aria-hidden="true" />
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
                                  type="button"
                                  className="pane-chrome-button pane-chrome-close"
                                  onClick={() => handleRemoveTerminalPane(id)}
                                  aria-label={t('session.removePane', { name: serverName })}
                                  title={t('session.removePane', { name: serverName })}
                                >
                                  <X className="h-3.5 w-3.5" aria-hidden="true" />
                                </button>,
                              ] : []}
                              className={cn('vibe-mosaic-pane--terminal', activeSessionId && id === sessionPaneId(activeSessionId) && 'mosaic-window-active')}
                            >
                              <PaneDropZone paneId={id}>
                                <div
                                  className="relative h-full"
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
                    {isPerformanceOpen && activeSession?.state === 'connected' && (
                      <PerformanceDock
                        key={activeSession.id}
                        session={activeSession}
                        plugin={plugins.find((plugin) => plugin.manifest.id === 'server-performance')}
                        onClose={() => setIsPerformanceOpen(false)}
                      />
                    )}
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

              {!activeSession?.container && <SftpPanel
                ref={sftpPanelRef}
                sessionId={activeSession?.id}
                sessionType={activeSession?.sessionType}
                defaultCollapsed={true}
                dock="right"
                onCollapsedChange={handleSftpCollapsedChange}
              />}
              {containerParentId && <ContainerPanel key={containerParentId} parentId={containerParentId} onConnected={handleConnected} onClose={() => setContainerParentId(null)} />}
              <WorkspaceChangesPanel
                open={isWorkspaceChangesOpen}
                cwd={activeSession?.purpose === 'coding_agent' ? activeSession.cwd : undefined}
                sessionName={activeSession?.purpose === 'coding_agent' ? activeSession.serverName : undefined}
                onClose={() => setIsWorkspaceChangesOpen(false)}
              />
                <AgentActivityPanel
                  open={isHistoryOpen}
                  onClose={() => setIsHistoryOpen(false)}
                  onSessionsChanged={handleGatewaySessionsChanged}
                  onSelectSession={handleConnected}
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

      <SelectServerDialog
        isOpen={isSelectServerOpen}
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
