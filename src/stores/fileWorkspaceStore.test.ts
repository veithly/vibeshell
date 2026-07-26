import { beforeEach, describe, expect, it } from 'vitest';
import { useFileWorkspaceStore } from './fileWorkspaceStore';

const firstFile = {
  sessionId: 'session-a',
  path: '/srv/app/main.ts',
  name: 'main.ts',
  size: 128,
};

describe('fileWorkspaceStore', () => {
  beforeEach(() => {
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
  });

  it('opens every file in a deduplicated active tab', () => {
    const store = useFileWorkspaceStore.getState();
    store.openFile(firstFile);
    store.openFile(firstFile);

    const state = useFileWorkspaceStore.getState();
    expect(state.tabs).toHaveLength(1);
    expect(state.activeTabId).toBe(state.tabs[0].id);
    expect(state.tabs[0]).toMatchObject({ ...firstFile, kind: 'text', dirty: false });
  });

  it('selects the neighboring file tab when the active tab closes', () => {
    const store = useFileWorkspaceStore.getState();
    store.openFile(firstFile);
    store.openFile({
      sessionId: 'session-a',
      path: '/srv/app/manual.pdf',
      name: 'manual.pdf',
      size: 2048,
    });

    const activeId = useFileWorkspaceStore.getState().activeTabId;
    expect(activeId).not.toBeNull();
    useFileWorkspaceStore.getState().closeTab(activeId!);

    const state = useFileWorkspaceStore.getState();
    expect(state.tabs).toHaveLength(1);
    expect(state.activeTabId).toBe(state.tabs[0].id);
  });

  it('tracks dirty editor state without replacing the tab identity', () => {
    useFileWorkspaceStore.getState().openFile(firstFile);
    const id = useFileWorkspaceStore.getState().activeTabId!;

    useFileWorkspaceStore.getState().setDirty(id, true);

    expect(useFileWorkspaceStore.getState().tabs[0]).toMatchObject({ id, dirty: true });
  });

  it('retires every file tab when its owning session is removed', () => {
    const store = useFileWorkspaceStore.getState();
    store.openFile(firstFile);
    store.openFile({ sessionId: 'session-b', path: '/tmp/keep.txt', name: 'keep.txt', size: 1 });
    store.activateTab(useFileWorkspaceStore.getState().tabs[0].id);

    store.closeTabsForSession('session-a');

    const state = useFileWorkspaceStore.getState();
    expect(state.tabs.map((tab) => tab.sessionId)).toEqual(['session-b']);
    expect(state.activeTabId).toBe(state.tabs[0].id);
  });

  it('retains only tabs whose owning sessions still exist', () => {
    const store = useFileWorkspaceStore.getState();
    store.openFile(firstFile);
    store.openFile({ sessionId: 'session-b', path: '/tmp/keep.txt', name: 'keep.txt', size: 1 });
    store.openFile({ sessionId: 'session-c', path: '/tmp/remove.txt', name: 'remove.txt', size: 1 });

    store.retainTabsForSessions(['session-b']);

    const state = useFileWorkspaceStore.getState();
    expect(state.tabs.map((tab) => tab.sessionId)).toEqual(['session-b']);
    expect(state.activeTabId).toBe(state.tabs[0].id);
  });
});
