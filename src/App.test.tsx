import type { ReactNode } from 'react';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';
import { usePluginWorkspaceStore } from './stores/pluginWorkspaceStore';

interface TestSession {
  id: string;
  serverId: string;
  serverName: string;
  state: 'connected';
  createdAt: number;
  sessionType: 'local';
  purpose: 'shell' | 'coding_agent';
  cwd?: string;
  agentId?: string;
}


const testState = vi.hoisted(() => ({
  sessions: [] as TestSession[],
  activeSessionId: null as string | null,
  compact: true,
  performanceInstalled: true,
  performanceEnabled: true,
  performanceInstallFails: false,
  capabilities: {
    platform: 'macos',
    isMobile: false,
    windowControls: true,
    localShell: true,
    agentGateway: true,
    desktopUpdater: false,
    cliIpc: true,
    directoryTransfer: true,
    backgroundTunnels: true,
  },
  actions: {
    setActiveSession: vi.fn(),
    killSession: vi.fn(async () => true),
    killLocalShellSession: vi.fn(async () => true),
    removeSession: vi.fn(),
    connectWithCredentials: vi.fn(async () => null),
    fetchSessions: vi.fn(async () => undefined),
    syncRemoteSessions: vi.fn(async () => undefined),
    createLocalShellSession: vi.fn(async () => null),
    initializeSettings: vi.fn(async () => undefined),
    loadRuntimeCapabilities: vi.fn(),
    fetchServers: vi.fn(async () => undefined),
    fetchGroups: vi.fn(async () => undefined),
    fetchPlugins: vi.fn(async () => undefined),
    checkForUpdates: vi.fn(async () => null),
    markVersionNotified: vi.fn(),
    goToMain: vi.fn(),
    goToSettings: vi.fn(),
    goToPlugins: vi.fn(),
    notifyWarning: vi.fn(),
    notifyError: vi.fn(),
    installPerformance: vi.fn(),
    enablePerformance: vi.fn(),
  },
}));

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

vi.mock('react-mosaic-component2', () => {
  const renderNode = (node: unknown, renderTile: (id: string, path: []) => ReactNode): ReactNode => {
    if (typeof node === 'string') return renderTile(node, []);
    if (node && typeof node === 'object' && 'first' in node && 'second' in node) {
      return (
        <>
          {renderNode((node as { first: unknown }).first, renderTile)}
          {renderNode((node as { second: unknown }).second, renderTile)}
        </>
      );
    }
    return null;
  };
  return {
    Mosaic: ({
      value,
      renderTile,
    }: {
      value: unknown;
      renderTile: (id: string, path: []) => ReactNode;
    }) => (
      <div
        data-testid="mosaic"
        data-mosaic-value={typeof value === 'string' ? value : 'branch'}
      >
        {renderNode(value, renderTile)}
      </div>
    ),
    MosaicWindow: ({
      title,
      toolbarControls = [],
      children,
    }: {
      title: string;
      toolbarControls?: ReactNode[];
      children: ReactNode;
    }) => (
      <section aria-label={`pane:${title}`}>
        <div data-testid="pane-toolbar">{toolbarControls}</div>
        {children}
      </section>
    ),
  };
});

vi.mock('./stores/sessionStore', () => {
  const getState = () => ({
    sessions: testState.sessions,
    activeSessionId: testState.activeSessionId,
    ...testState.actions,
  });
  return {
    useSessionStore: Object.assign(() => getState(), { getState }),
  };
});

vi.mock('./stores/navigationStore', () => ({
  useNavigationStore: () => ({
    currentView: 'main',
    goToMain: testState.actions.goToMain,
    goToSettings: testState.actions.goToSettings,
    goToPlugins: testState.actions.goToPlugins,
  }),
}));

vi.mock('./stores/notificationStore', () => ({
  useNotificationStore: () => ({
    warning: testState.actions.notifyWarning,
    error: testState.actions.notifyError,
  }),
}));

vi.mock('./stores/settingsStore', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./stores/settingsStore')>();
  const getState = () => ({
    settings: {
      appearance: {
        theme: 'test-theme',
        themeMode: 'dark',
        lightTheme: 'paper-white',
        darkTheme: 'test-theme',
      },
    },
    initializeSettings: testState.actions.initializeSettings,
    updateAppearanceSettings: vi.fn(),
  });
  const useSettingsStore = (selector?: (state: ReturnType<typeof getState>) => unknown) =>
    selector ? selector(getState()) : getState();
  return {
    ...actual,
    useSettingsStore: Object.assign(useSettingsStore, { getState }),
    themes: [{
      name: 'test-theme',
      colors: {
        bg: '#000000',
        bgDark: '#000000',
        bgHl: '#111111',
        fg: '#ffffff',
        fgDark: '#aaaaaa',
        accent: '#00aaff',
        onAccent: '#000000',
        red: '#ff0000',
        green: '#00ff00',
        yellow: '#ffff00',
        magenta: '#ff00ff',
        cyan: '#00ffff',
        orange: '#ff8800',
      },
    }],
  };
});

vi.mock('./stores/updateStore', () => {
  const useUpdateStore = Object.assign(
    () => ({
      checkForUpdates: testState.actions.checkForUpdates,
      markVersionNotified: testState.actions.markVersionNotified,
    }),
    { getState: () => ({ lastNotifiedVersion: null }) }
  );
  return { UPDATE_CHECK_INTERVAL_MS: 60_000, useUpdateStore };
});

vi.mock('./stores/serverStore', () => {
  const getState = () => ({
    servers: [],
    fetchServers: testState.actions.fetchServers,
    fetchGroups: testState.actions.fetchGroups,
  });
  const useServerStore = (selector: (state: ReturnType<typeof getState>) => unknown) => (
    selector(getState())
  );
  return { useServerStore: Object.assign(useServerStore, { getState }) };
});

vi.mock('./stores/runtimeCapabilitiesStore', () => {
  const getState = () => ({
    capabilities: testState.capabilities,
    load: testState.actions.loadRuntimeCapabilities,
  });
  return {
    useRuntimeCapabilitiesStore: (
      selector: (state: ReturnType<typeof getState>) => unknown
    ) => selector(getState()),
  };
});

vi.mock('./stores/pluginStore', () => {
  const dockerManifest = {
    schemaVersion: 1,
    id: 'docker-containers',
    name: 'Docker Containers',
    description: 'Inspect containers',
    version: '1.2.0',
    author: 'VibeShell',
    category: 'containers',
    icon: 'box',
    permissions: ['remote_exec'],
    sessionTypes: ['ssh', 'local'],
    defaultSettings: {},
    entry: {
      type: 'commands',
      actions: [{
        id: 'containers',
        name: 'Containers',
        description: 'List containers',
        program: 'docker',
        args: ['ps', '--all'],
        inputs: [],
        requiresConfirmation: false,
        elevate: false,
        allowSudo: true,
        output: { kind: 'text', columns: [], delimiter: '\t' },
      }],
    },
  };
  const getState = () => ({
    plugins: [{
      manifest: dockerManifest,
      source: 'builtin',
      installed: true,
      enabled: true,
      grantedPermissions: ['remote_exec'],
      settings: {},
      installedAt: 1,
    }, {
      manifest: {
        ...dockerManifest,
        id: 'server-performance',
        name: 'Server Performance',
        icon: 'activity',
        permissions: ['local_system_read'],
        entry: { type: 'native', view: 'server-status' },
      },
      source: 'builtin',
      installed: testState.performanceInstalled,
      enabled: testState.performanceEnabled,
      grantedPermissions: ['local_system_read'],
      settings: {},
      installedAt: 1,
    }],
    loading: false,
    initialized: true,
    operationId: null,
    error: null,
    fetchPlugins: testState.actions.fetchPlugins,
    installPlugin: async () => {
      testState.actions.installPerformance();
      if (testState.performanceInstallFails) return false;
      testState.performanceInstalled = true;
      testState.performanceEnabled = true;
      return true;
    },
    setPluginEnabled: async () => {
      testState.actions.enablePerformance();
      testState.performanceEnabled = true;
      return true;
    },
    executePluginAction: vi.fn(async () => null),
    updatePluginSettings: vi.fn(async () => true),
    clearError: vi.fn(),
  });
  return {
    usePluginStore: Object.assign(
      (selector?: (state: ReturnType<typeof getState>) => unknown) =>
        selector ? selector(getState()) : getState(),
      { getState }
    ),
  };
});

vi.mock('./lib/useMediaQuery', () => ({
  useMediaQuery: () => testState.compact,
}));

vi.mock('./lib/tauri', () => ({
  safeInvoke: vi.fn(async () => ({
    success: false,
    error: { message: 'not available in App test' },
  })),
}));

vi.mock('./components/SessionTabs', () => ({
  SessionTabs: ({ rightActions }: { rightActions: ReactNode }) => <div>{rightActions}</div>,
}));


vi.mock('./components/TitleBar', () => ({ TitleBar: ({ children }: { children?: import('react').ReactNode }) => <header>{children}</header> }));
vi.mock('./components/AddServerDialog', () => ({ AddServerDialog: () => null }));
vi.mock('./components/EditServerDialog', () => ({ EditServerDialog: () => null }));
vi.mock('./components/ConnectDialog', () => ({ ConnectDialog: () => null }));
vi.mock('./components/SelectServerDialog', () => ({ SelectServerDialog: () => null }));
vi.mock('./components/ConfirmDialog', () => ({ ConfirmDialog: () => null }));
vi.mock('./components/Notifications', () => ({ Notifications: () => null }));
vi.mock('./components/AgentActivityPanel', () => ({ AgentActivityPanel: () => null }));
vi.mock('./components/AgentApprovalDialog', () => ({ AgentApprovalDialog: () => null }));
vi.mock('./components/WorkspaceChangesPanel', () => ({ WorkspaceChangesPanel: () => null }));
vi.mock('./components/SessionPluginDock', () => ({ SessionPluginDock: () => null }));
vi.mock('./components/SnippetManager/SnippetManagerDialog', () => ({
  SnippetManagerDialog: () => null,
}));
vi.mock('./components/TunnelPanel/TunnelPanelDialog', () => ({
  TunnelPanelDialog: () => null,
}));
vi.mock('./components/FingerprintDialog', () => ({
  FingerprintVerificationDialog: () => null,
  FingerprintManagerDialog: () => null,
}));
vi.mock('./components/SftpPanel', async () => {
  const { forwardRef } = await import('react');
  return { SftpPanel: forwardRef((_props, _ref) => null) };
});
vi.mock('./components/Settings', () => ({ Settings: () => null }));
vi.mock('./components/ServerStatus', async (importOriginal) => ({
  ...await importOriginal<typeof import('./components/ServerStatus')>(),
  ServerStatus: ({ sessionId, onClose }: { sessionId: string; onClose?: () => void }) => <div data-testid="performance-metrics" data-session-id={sessionId}>CPU / Memory<button onClick={onClose}>Close metrics</button></div>,
}));
vi.mock('./components/PluginMarketplace', () => ({ PluginMarketplace: () => null }));
vi.mock('./components/Terminal', async () => {
  const { forwardRef } = await import('react');
  return {
    Terminal: forwardRef(({ sessionId }: { sessionId: string }, _ref) => (
      <div data-testid={`terminal-${sessionId}`} />
    )),
  };
});

function codingAgentSession(overrides: Partial<TestSession> = {}): TestSession {
  return {
    id: 'session-1',
    serverId: 'codex',
    serverName: 'Codex',
    state: 'connected',
    createdAt: 1,
    sessionType: 'local',
    purpose: 'coding_agent',
    cwd: '/workspace',
    agentId: 'codex',
    ...overrides,
  };
}

describe('App workspace actions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    testState.sessions = [codingAgentSession()];
    testState.activeSessionId = 'session-1';
    testState.compact = true;
    testState.performanceInstalled = true;
    testState.performanceEnabled = true;
    testState.performanceInstallFails = false;
    usePluginWorkspaceStore.setState({ tabs: [], activeTabId: null, panels: {} });
    testState.capabilities = {
      platform: 'macos',
      isMobile: false,
      windowControls: true,
      localShell: true,
      agentGateway: true,
      desktopUpdater: false,
      cliIpc: true,
      directoryTransfer: true,
      backgroundTunnels: true,
    };
    testState.actions.loadRuntimeCapabilities.mockImplementation(async () => (
      testState.capabilities
    ));
    vi.spyOn(console, 'log').mockImplementation(() => undefined);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('does not expose a remove control for the sole Mosaic pane', async () => {
    render(<App />);

    const mosaic = await screen.findByTestId('mosaic');
    expect(mosaic).toHaveAttribute('data-mosaic-value', 'session:session-1');

    const toolbar = await screen.findByTestId('pane-toolbar');
    expect(toolbar).toBeEmptyDOMElement();
    expect(
      within(toolbar).queryByRole('button', { name: 'session.removePane' })
    ).not.toBeInTheDocument();
  });

  it('opens a plugin without the removed split buttons', async () => {
    testState.compact = false;
    render(<App />);

    // The toolbar launcher lists plugins compatible with the active session.
    const launcher = await screen.findByRole('button', { name: 'common.more' });
    expect(launcher).toBeEnabled();
    fireEvent.click(launcher);

    const pluginItem = await screen.findByRole('menuitem', { name: 'plugins.catalog.docker-containers.name' });
    fireEvent.click(pluginItem);

    expect(await screen.findByText('Docker')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'plugins.splitRight' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'plugins.splitDown' })).not.toBeInTheDocument();
    expect(screen.getByTestId('terminal-session-1')).toBeInTheDocument();
    expect(screen.getAllByText(/Containers/).length).toBeGreaterThan(0);
    // Power users can still drop into the raw runner.
    fireEvent.click(screen.getByText('plugins.advancedMode ⌄'));
    expect(screen.getAllByText('Containers').length).toBeGreaterThan(0);
  });

  it('exposes one history action independent of gateway availability', async () => {
    const { rerender } = render(<App />);

    expect(await screen.findByRole('button', {name: 'sidebar.history'})).toBeEnabled();
    expect(screen.getByRole('button', {name: 'sidebar.sftp'})).toBeEnabled();
    expect(screen.getByRole('button', {name: 'sidebar.performance'})).toBeEnabled();
    fireEvent.click(screen.getByRole('button', {name: 'common.more'}));
    expect(screen.queryByRole('menuitem', {name: 'sidebar.history'})).not.toBeInTheDocument();
    for (const name of ['settings.dayMode', 'settings.nightMode', 'settings.themeModeSystem', 'sidebar.sftp', 'plugins.catalog.server-performance.name', 'session.splitHorizontal', 'session.splitVertical', 'session.closeSplits']) {
      expect(screen.queryByRole('menuitem', {name})).not.toBeInTheDocument();
    }
    for (const name of ['codingAgent.start', 'agentActivity.title', 'sidebar.quickCmd']) {
      expect(screen.queryByRole('menuitem', {name})).not.toBeInTheDocument();
    }
    expect(screen.getByRole('menuitem', {name: 'workspaceChanges.title'})).toBeEnabled();
    testState.sessions = [codingAgentSession({cwd: undefined})];
    testState.capabilities = {...testState.capabilities, localShell: false, agentGateway: false};
    rerender(<App />);
    expect(screen.getByRole('menuitem', {name: 'workspaceChanges.title'})).toBeDisabled();
    expect(screen.getByRole('button', {name: 'sidebar.history'})).toBeEnabled();
  });

  it('opens performance below the terminal without creating a workspace tab, then toggles closed', async () => {
    render(<App />);
    fireEvent.click(await screen.findByRole('button', {name: 'sidebar.performance'}));
    expect(await screen.findByTestId('performance-metrics')).toBeInTheDocument();
    expect(testState.actions.installPerformance).not.toHaveBeenCalled();
    expect(usePluginWorkspaceStore.getState().tabs).toHaveLength(0);
    const terminal = screen.getByTestId('terminal-session-1');
    const dock = screen.getByTestId('performance-dock');
    expect(terminal.compareDocumentPosition(dock) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(terminal.closest('.hidden')).toBeNull();
    expect(screen.getByRole('button', {name: 'sidebar.performance'})).toHaveAttribute('aria-pressed', 'true');
    fireEvent.click(screen.getByRole('button', {name: 'sidebar.performance'}));
    expect(screen.queryByTestId('performance-dock')).not.toBeInTheDocument();
    expect(screen.getByTestId('terminal-session-1')).toBeInTheDocument();
  });

  it('keeps top-level actions icon-only with tooltips and accessible names', async () => {
    render(<App />);
    for (const name of ['sidebar.sftp', 'sidebar.performance', 'sidebar.history', 'common.more']) {
      const button = await screen.findByRole('button', { name });
      expect(button.textContent).toBe('');
      expect(button).toHaveAttribute('data-tooltip', name);
      expect(button.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');
    }
  });

  it('closes only the performance pane and follows a changed active session', async () => {
    const { rerender } = render(<App />);
    fireEvent.click(await screen.findByRole('button', {name: 'sidebar.performance'}));
    await screen.findByTestId('performance-metrics');
    testState.sessions = [codingAgentSession({id: 'session-2'})];
    testState.activeSessionId = 'session-2';
    rerender(<App />);
    expect(screen.getByTestId('performance-metrics')).toHaveAttribute('data-session-id', 'session-2');
    fireEvent.click(screen.getByText('Close metrics'));
    expect(screen.queryByTestId('performance-dock')).not.toBeInTheDocument();
    expect(testState.actions.killSession).not.toHaveBeenCalled();
    expect(testState.actions.killLocalShellSession).not.toHaveBeenCalled();
  });

  it('enables the bundled performance feature on an explicit first-use click', async () => {
    testState.performanceInstalled = false;
    testState.performanceEnabled = false;
    render(<App />);
    expect(testState.actions.installPerformance).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole('button', {name: 'sidebar.performance'}));
    expect(await screen.findByTestId('performance-metrics')).toBeInTheDocument();
    expect(testState.actions.installPerformance).toHaveBeenCalledOnce();
  });

  it('surfaces a failed performance install instead of opening a broken tab', async () => {
    testState.performanceInstalled = false;
    testState.performanceInstallFails = true;
    render(<App />);
    fireEvent.click(await screen.findByRole('button', {name: 'sidebar.performance'}));
    await waitFor(() => expect(testState.actions.notifyError).toHaveBeenCalled());
    expect(usePluginWorkspaceStore.getState().tabs).toHaveLength(0);
  });
});
