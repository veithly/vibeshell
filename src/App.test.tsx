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

interface TestMenuItem {
  id: string;
  label: string;
  disabled?: boolean;
  pressed?: boolean;
  onSelect: () => void;
}

const testState = vi.hoisted(() => ({
  sessions: [] as TestSession[],
  activeSessionId: null as string | null,
  compact: true,
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

vi.mock('./stores/settingsStore', () => {
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
    }],
    loading: false,
    initialized: true,
    operationId: null,
    error: null,
    fetchPlugins: testState.actions.fetchPlugins,
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

vi.mock('./components/MobileWorkspaceActions', () => ({
  MobileWorkspaceActions: ({ menuItems }: { menuItems: TestMenuItem[] }) => (
    <div data-testid="mobile-workspace-actions">
      {menuItems.map((item) => (
        <button
          key={item.id}
          data-testid={`mobile-action-${item.id}`}
          disabled={item.disabled}
          aria-pressed={item.pressed}
          onClick={item.onSelect}
        >
          {item.label}
        </button>
      ))}
    </div>
  ),
}));

vi.mock('./components/TitleBar', () => ({ TitleBar: () => null }));
vi.mock('./components/AddServerDialog', () => ({ AddServerDialog: () => null }));
vi.mock('./components/EditServerDialog', () => ({ EditServerDialog: () => null }));
vi.mock('./components/ConnectDialog', () => ({ ConnectDialog: () => null }));
vi.mock('./components/SelectServerDialog', () => ({ SelectServerDialog: () => null }));
vi.mock('./components/QuickCommandDialog', () => ({ QuickCommandDialog: () => null }));
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

  it('opens a plugin as a workspace tab and splits it beside the terminal', async () => {
    testState.compact = false;
    render(<App />);

    // The toolbar launcher lists plugins compatible with the active session.
    const launcher = await screen.findByRole('button', { name: 'plugins.openTabLauncher' });
    expect(launcher).toBeEnabled();
    fireEvent.click(launcher);

    const pluginItem = await screen.findByRole('menuitem');
    fireEvent.click(pluginItem);

    // The plugin takes over the workspace area and offers split controls.
    const splitButton = await screen.findByRole('button', { name: 'plugins.splitRight' });
    expect(splitButton).toBeInTheDocument();
    expect(screen.getByText('Codex')).toBeInTheDocument();

    // Splitting pins the plugin beside the terminal inside the mosaic.
    fireEvent.click(splitButton);
    await waitFor(() => {
      expect(
        screen.getByLabelText('pane:plugins.catalog.docker-containers.name · Codex')
      ).toBeInTheDocument();
    });
    expect(screen.getByTestId('terminal-session-1')).toBeInTheDocument();

    // The built-in management dashboard replaces the raw action form.
    const pluginPane = screen.getByLabelText('pane:plugins.catalog.docker-containers.name · Codex');
    expect(within(pluginPane).getByText('Docker')).toBeInTheDocument();
    expect(within(pluginPane).getAllByText(/Containers/).length).toBeGreaterThan(0);
    // Power users can still drop into the raw runner.
    fireEvent.click(within(pluginPane).getByText('plugins.advancedMode ⌄'));
    expect(within(pluginPane).getAllByText('Containers').length).toBeGreaterThan(0);
  });

  it('keeps compact coding-agent, workspace-change, and gateway actions distinct', async () => {
    const { rerender } = render(<App />);

    expect(await screen.findByTestId('mobile-action-coding-agent')).toHaveTextContent(
      'codingAgent.start'
    );
    expect(screen.getByTestId('mobile-action-workspace-changes')).toHaveTextContent(
      'workspaceChanges.title'
    );
    expect(screen.getByTestId('mobile-action-workspace-changes')).toBeEnabled();
    expect(screen.getByTestId('mobile-action-agent-activity')).toHaveTextContent(
      'agentActivity.title'
    );

    testState.sessions = [codingAgentSession({ cwd: undefined })];
    rerender(<App />);
    expect(screen.getByTestId('mobile-action-workspace-changes')).toBeDisabled();

    testState.sessions = [codingAgentSession({ purpose: 'shell', cwd: '/workspace' })];
    testState.capabilities = {
      ...testState.capabilities,
      localShell: false,
      agentGateway: false,
    };
    rerender(<App />);

    expect(screen.queryByTestId('mobile-action-coding-agent')).not.toBeInTheDocument();
    expect(screen.getByTestId('mobile-action-workspace-changes')).toBeDisabled();
    expect(screen.queryByTestId('mobile-action-agent-activity')).not.toBeInTheDocument();
  });
});
