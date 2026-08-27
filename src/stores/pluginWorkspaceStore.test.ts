import { beforeEach, describe, expect, it } from 'vitest';
import {
  pluginPanelKey,
  usePluginWorkspaceStore,
  type OpenPluginTabInput,
} from './pluginWorkspaceStore';

const input: OpenPluginTabInput = {
  pluginId: 'docker-containers',
  sessionId: 'session-1',
  sessionType: 'ssh',
  serverName: 'JP',
};

describe('pluginWorkspaceStore', () => {
  beforeEach(() => {
    usePluginWorkspaceStore.setState({ tabs: [], activeTabId: null, panels: {} });
  });

  it('opens a tab once per session/plugin pair and activates it', () => {
    const store = usePluginWorkspaceStore.getState();

    store.openPluginTab(input);
    store.openPluginTab(input);

    const state = usePluginWorkspaceStore.getState();
    expect(state.tabs).toHaveLength(1);
    expect(state.tabs[0].id).toBe(pluginPanelKey('session-1', 'docker-containers'));
    expect(state.activeTabId).toBe('session-1::docker-containers');
  });

  it('keeps separate tabs for the same plugin across sessions', () => {
    const store = usePluginWorkspaceStore.getState();
    store.openPluginTab(input);
    store.openPluginTab({ ...input, sessionId: 'session-2' });

    expect(usePluginWorkspaceStore.getState().tabs).toHaveLength(2);
  });

  it('activates the neighbouring tab on close', () => {
    const store = usePluginWorkspaceStore.getState();
    store.openPluginTab(input);
    store.openPluginTab({ ...input, pluginId: 'redis-inspector' });

    usePluginWorkspaceStore.getState().closeTab('session-1::redis-inspector');

    const state = usePluginWorkspaceStore.getState();
    expect(state.tabs.map((tab) => tab.pluginId)).toEqual(['docker-containers']);
    expect(state.activeTabId).toBe('session-1::docker-containers');
  });

  it('closes every tab and panel state for a session', () => {
    const store = usePluginWorkspaceStore.getState();
    store.openPluginTab(input);
    store.setActiveAction('session-1::docker-containers', 'containers');
    store.setResult('session-1::docker-containers', {
      pluginId: 'docker-containers',
      actionId: 'containers',
      output: 'ok',
      durationMs: 5,
      truncated: false,
    });

    usePluginWorkspaceStore.getState().closeTabsForSession('session-1');

    const state = usePluginWorkspaceStore.getState();
    expect(state.tabs).toHaveLength(0);
    expect(state.panels).toEqual({});
    expect(state.activeTabId).toBeNull();
  });

  it('resets inputs and results when the active action changes', () => {
    const key = 'session-1::docker-containers';
    const store = usePluginWorkspaceStore.getState();
    store.setInputValue(key, 'container', 'web');
    store.setActiveAction(key, 'containers');

    const panel = usePluginWorkspaceStore.getState().panels[key];
    expect(panel).toEqual({ activeActionId: 'containers', inputValues: {}, result: null });
  });
});
