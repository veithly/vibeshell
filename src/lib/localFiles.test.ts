import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
const invoke = vi.hoisted(() => vi.fn());
vi.mock('./tauri', () => ({ safeInvoke: invoke }));
import { openLocalFiles } from './localFiles';
import { useFileWorkspaceStore, LOCAL_FILE_ORIGIN } from '../stores/fileWorkspaceStore';
import { useSessionStore } from '../stores/sessionStore';
import { captureTransfer, hydrateTransfer, useDetachedOwnership } from './detach';
import { remapWorkspace, saveWorkspaceLayout, readWorkspaceLayout } from './workspacePersistence';
import { filePaneId } from './paneIds';

describe('standalone local documents', () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => values.set(key, value), removeItem: (key: string) => values.delete(key) });
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
    useSessionStore.setState({ sessions: [], activeSessionId: null });
    useDetachedOwnership.setState({ owners: {} });
    invoke.mockReset().mockImplementation(async (command: string, args?: { path?: string }) => command === 'local_file_stat'
      ? { success: true, data: { path: args!.path, name: args!.path!.split('/').pop(), size: 12 } }
      : { success: true, data: [] });
  });
  afterEach(() => vi.unstubAllGlobals());
  it('opens Markdown, SVG, text, images and extensionless text without a session', async () => {
    await openLocalFiles(['/tmp/note.md', '/tmp/vector.svg', '/tmp/note.txt', '/tmp/photo.png', '/tmp/scratch']);
    expect(useFileWorkspaceStore.getState().tabs.map(tab => tab.kind)).toEqual(['text', 'image', 'text', 'image', 'text']);
    expect(useFileWorkspaceStore.getState().tabs.every(tab => tab.source === 'local')).toBe(true);
    expect(useSessionStore.getState().sessions).toHaveLength(0);
    expect(invoke.mock.calls.every(([command]) => command === 'local_file_stat')).toBe(true);
  });
  it('deduplicates repeated paths and keeps local files when all terminals disappear', async () => {
    await openLocalFiles(['/tmp/note.md', '/tmp/note.md']); await openLocalFiles(['/tmp/note.md']);
    useFileWorkspaceStore.getState().retainTabsForSessions([]);
    expect(useFileWorkspaceStore.getState().tabs).toHaveLength(1);
  });
  it('cancellation does not change tabs and the picker can open again', async () => {
    await openLocalFiles(); await openLocalFiles();
    expect(invoke).toHaveBeenCalledTimes(2);
    expect(useFileWorkspaceStore.getState().tabs).toHaveLength(0);
  });
  it('preserves local file identity through save and session remapping', async () => {
    await openLocalFiles(['/tmp/note.md']); const tab = useFileWorkspaceStore.getState().tabs[0];
    saveWorkspaceLayout(filePaneId(tab.id), filePaneId(tab.id));
    const saved = readWorkspaceLayout()!; const restored = remapWorkspace(saved, new Map());
    expect(restored.files).toEqual([tab]); expect(restored.tree).toBe(filePaneId(tab.id));
    expect(restored.activeFileId).toBe(tab.id);
  });
  it('can hand off and restore a file without fabricating a shell session', async () => {
    await openLocalFiles(['/tmp/note.md']);
    const target = { kind: 'file' as const, source: 'local' as const, sessionId: LOCAL_FILE_ORIGIN, path: '/tmp/note.md', name: 'note.md', size: 12 };
    const transfer = captureTransfer(target); expect(transfer.session).toBeUndefined();
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null }); hydrateTransfer(transfer);
    expect(useFileWorkspaceStore.getState().tabs).toHaveLength(1);
    expect(useSessionStore.getState().sessions).toHaveLength(0);
    expect(() => hydrateTransfer({ ...transfer, file: { ...transfer.file!, path: '/another-file.md' } })).toThrow('Invalid local file');
  });
});
