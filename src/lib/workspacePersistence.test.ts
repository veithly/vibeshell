// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { readWorkspaceLayout, remapWorkspace, saveWorkspaceLayout, type WorkspaceLayout } from './workspacePersistence';
import { useSessionStore, type Session } from '../stores/sessionStore';
import { useFileWorkspaceStore } from '../stores/fileWorkspaceStore';
import { usePluginWorkspaceStore } from '../stores/pluginWorkspaceStore';
import { filePaneId, pluginPaneId, sessionPaneId } from './paneIds';
import { getLeaves } from './mosaicTree';

const session: Session = { id: 'new', serverId: 'server', serverName: 'Host', sessionType: 'ssh', state: 'disconnected', createdAt: 1788739200000, purpose: 'shell' };
function fixture(): WorkspaceLayout {
  const file = { id: 'old\u0000/srv/file.ts', sessionId: 'old', path: '/srv/file.ts', name: 'file.ts', size: 12, kind: 'text' as const, dirty: true };
  const plugin = { id: 'old::docker', sessionId: 'old', pluginId: 'docker', serverName: 'Host', sessionType: 'ssh' as const };
  return { version: 2,
    tree: { direction: 'row', first: sessionPaneId('old'), splitPercentage: 42.75,
      second: { direction: 'column', first: filePaneId(file.id), second: pluginPaneId(plugin.id), splitPercentage: 27 } },
    focusedPane: filePaneId(file.id), sessions: [{ ...session, id: 'old' }], files: [file], plugins: [plugin],
    activeSessionId: 'old', activeFileId: file.id, activePluginId: null,
    detached: [{ target: { kind: 'terminal', sessionId: 'old', title: 'Host' }, geometryKey: 'stable-screen-position' }],
  };
}

describe('persistent workspace identity', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    const values = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) });
    useSessionStore.setState({ sessions: [], activeSessionId: null });
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
    usePluginWorkspaceStore.setState({ tabs: [], activeTabId: null, panels: {} });
  });
  it('remaps all session, file, plugin, focus and detached references while preserving ratios', () => {
    const next = remapWorkspace(fixture(), new Map([['old', session]]));
    expect(getLeaves(next.tree)).toEqual([sessionPaneId('new'), filePaneId('new\u0000/srv/file.ts'), pluginPaneId('new::docker')]);
    expect(next.tree).toMatchObject({ splitPercentage: 42.75, second: { splitPercentage: 27 } });
    expect(next.focusedPane).toBe(filePaneId('new\u0000/srv/file.ts'));
    expect(next.activeFileId).toBe('new\u0000/srv/file.ts');
    expect(next.activeSessionId).toBe('new');
    expect(next.detached[0]).toMatchObject({ target: { sessionId: 'new' }, geometryKey: 'stable-screen-position' });
  });
  it('collapses unavailable sessions instead of restoring dead references', () => {
    const next = remapWorkspace(fixture(), new Map());
    expect(next.tree).toBeNull(); expect(next.files).toEqual([]); expect(next.plugins).toEqual([]); expect(next.detached).toEqual([]);
    expect(next.focusedPane).toBeNull(); expect(next.activeFileId).toBeNull();
  });
  it('saves tab order and split layout without credentials or plugin execution inputs', () => {
    const original = fixture();
    useSessionStore.setState({ sessions: [{ ...session, password: 'never-store-password' } as Session], activeSessionId: 'new' });
    useFileWorkspaceStore.setState({ tabs: original.files, activeTabId: original.activeFileId });
    usePluginWorkspaceStore.setState({ tabs: original.plugins, panels: { 'old::docker': { inputValues: { command: 'never-reexecute-this' } } as never } });
    saveWorkspaceLayout(original.tree, original.focusedPane);
    const raw = localStorage.getItem('vibeshell.workspace-layout.v2')!;
    expect(raw).not.toContain('never-store-password'); expect(raw).not.toContain('never-reexecute-this');
    expect(readWorkspaceLayout()).toMatchObject({ tree: original.tree, files: original.files, plugins: original.plugins, activeFileId: original.activeFileId });
  });
  it.each(['invalid-json', '{}', 'null', '{"version":1}', '{"version":2,"sessions":[]}'])('ignores invalid persisted data %s', (raw) => {
    localStorage.setItem('vibeshell.workspace-layout.v2', raw);
    expect(readWorkspaceLayout()).toBeNull();
  });
});
