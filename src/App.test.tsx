import type { ReactNode } from 'react';
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App';

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

vi.mock('react-mosaic-component2', () => ({
  Mosaic: ({
    value,
    renderTile,
  }: {
    value: unknown;
    renderTile: (id: string, path: []) => ReactNode;
  }) => (
    <div
      data-testid="mosaic"
      data-mosaic-value={typeof value === 'string' ? value : ''}
    >
      {typeof value === 'string' ? renderTile(value, []) : null}
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
}));

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

vi.mock('./stores/settingsStore', () => ({
  useSettingsStore: () => ({
    settings: { appearance: { theme: 'test-theme' } },
    initializeSettings: testState.actions.initializeSettings,
  }),
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
}));

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
  const getState = () => ({ fetchPlugins: testState.actions.fetchPlugins });
  return {
    usePluginStore: (selector: (state: ReturnType<typeof getState>) => unknown) => (
      selector(getState())
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
    expect(mosaic).toHaveAttribute('data-mosaic-value', 'session-1');

    const toolbar = await screen.findByTestId('pane-toolbar');
    expect(toolbar).toBeEmptyDOMElement();
    expect(
      within(toolbar).queryByRole('button', { name: 'session.removePane' })
    ).not.toBeInTheDocument();
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
