import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const invoke = vi.hoisted(() => vi.fn());
vi.mock('../../lib/tauri', () => ({ safeInvoke: invoke }));
vi.mock('react-i18next', () => {
  const t = (key: string) => key;
  return { useTranslation: () => ({ t }) };
});
import { FileWorkspace } from './FileWorkspace';
import { forgetTextBuffer } from '../../lib/fileEditBuffer';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../../stores/fileWorkspaceStore';
const tab: FileWorkspaceTab = { source: 'local', sessionId: 'local-files', id: 'local-files\u0000/tmp/note.md', name: 'note.md', path: '/tmp/note.md', size: 10, kind: 'text', dirty: false };
describe('local document workspace', () => {
  beforeEach(() => {
    forgetTextBuffer(tab.id); useFileWorkspaceStore.setState({ tabs: [tab], activeTabId: tab.id });
    invoke.mockReset().mockImplementation(async (command: string) => command === 'local_file_read'
      ? { success: true, data: { content: '# Original', isBinary: false, truncated: false, size: 10, mimeType: 'text/markdown' } }
      : { success: true, data: null });
  });
  afterEach(() => { cleanup(); forgetTextBuffer(tab.id); });
  it('starts in preview and saves local source with the original disk revision', async () => {
    render(<FileWorkspace tab={tab} isActive />);
    expect(await screen.findByRole('heading', { level: 1 })).toHaveTextContent('Original');
    fireEvent.click(screen.getByRole('button', { name: 'localFiles.split' }));
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(editor, { target: { value: '# Edited' } });
    fireEvent.keyDown(editor, { key: 's', metaKey: true });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('local_file_write', { request: { sessionId: 'local-files', path: '/tmp/note.md', content: '# Edited', expectedContent: '# Original' } }));
    expect(invoke.mock.calls.some(([command]) => command.startsWith('sftp_'))).toBe(false);
  });
  it('keeps a conflicting local edit dirty instead of pretending it was saved', async () => {
    render(<FileWorkspace tab={tab} isActive />);
    await screen.findByRole('heading', { level: 1 }); fireEvent.click(screen.getByRole('button', { name: 'localFiles.source' }));
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(editor, { target: { value: '# My draft' } });
    invoke.mockResolvedValue({ success: false, error: { message: 'File changed on disk' } });
    fireEvent.keyDown(editor, { key: 's', metaKey: true });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('local_file_write', expect.anything()));
    expect(useFileWorkspaceStore.getState().tabs[0].dirty).toBe(true);
    expect(editor).toHaveValue('# My draft');
  });
});
