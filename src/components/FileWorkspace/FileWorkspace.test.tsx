import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { FileWorkspace } from './FileWorkspace';
import { forgetTextBuffer, readTextBuffer, setTextBufferLocked } from '../../lib/fileEditBuffer';
import { useFileWorkspaceStore, type FileWorkspaceTab } from '../../stores/fileWorkspaceStore';
import { useRuntimeCapabilitiesStore } from '../../stores/runtimeCapabilitiesStore';

const safeInvokeMock = vi.fn();

vi.mock('../../lib/tauri', () => ({
  safeInvoke: (...args: unknown[]) => safeInvokeMock(...args),
}));

const tab: FileWorkspaceTab = {
  id: 'session-1\0/srv/main.ts',
  sessionId: 'session-1',
  path: '/srv/main.ts',
  name: 'main.ts',
  size: 14,
  kind: 'text',
  dirty: false,
};

describe('FileWorkspace', () => {
  beforeEach(() => {
    forgetTextBuffer(tab.id);
    setTextBufferLocked(tab.id, false);
    useFileWorkspaceStore.setState({ tabs: [tab], activeTabId: tab.id });
    useRuntimeCapabilitiesStore.setState((state) => ({
      ...state,
      capabilities: { ...state.capabilities, directoryTransfer: false },
    }));
    safeInvokeMock.mockReset();
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_read_file') {
        return Promise.resolve({
          success: true,
          data: {
            content: 'const value = 1;',
            isBinary: false,
            size: 16,
            truncated: false,
            mimeType: 'text/typescript',
          },
        });
      }
      return Promise.resolve({ success: true, data: null });
    });
  });

  afterEach(() => { cleanup(); forgetTextBuffer(tab.id); });

  it('keeps unsaved edits when a file is moved into a split and remounted', async () => {
    const first = render(<FileWorkspace tab={tab} isActive />);
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(editor, { target: { value: 'unsaved draft after moving' } });
    first.unmount();
    safeInvokeMock.mockClear();
    render(<FileWorkspace tab={useFileWorkspaceStore.getState().tabs[0]} isActive />);
    expect(await screen.findByRole('textbox', { name: 'File editor' })).toHaveValue('unsaved draft after moving');
    expect(safeInvokeMock).not.toHaveBeenCalled();
    expect(readTextBuffer(tab.id)?.saved).toBe('const value = 1;');
  });

  it('makes an outgoing file read-only until the handoff completes or fails', async () => {
    render(<FileWorkspace tab={tab} isActive />);
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    act(() => setTextBufferLocked(tab.id, true));
    expect(editor).toHaveAttribute('readonly');
    fireEvent.change(editor, { target: { value: 'must not overwrite transferred content' } });
    expect(readTextBuffer(tab.id)?.text).toBe('const value = 1;');
    act(() => setTextBufferLocked(tab.id, false));
    expect(editor).not.toHaveAttribute('readonly');
  });

  it('loads, highlights, edits, and saves a text file from its active tab', async () => {
    render(<FileWorkspace tab={tab} isActive />);

    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    expect(editor).toHaveValue('const value = 1;');
    expect(document.querySelector('.hljs-keyword')).toHaveTextContent('const');

    fireEvent.change(editor, { target: { value: 'const value = 2;' } });
    expect(useFileWorkspaceStore.getState().tabs[0].dirty).toBe(true);

    const saveEvent = new KeyboardEvent('keydown', {
      key: 's',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    });
    editor.dispatchEvent(saveEvent);

    expect(saveEvent.defaultPrevented).toBe(true);
    await waitFor(() => {
      expect(safeInvokeMock).toHaveBeenCalledWith('sftp_write_file', {
        request: {
          sessionId: 'session-1',
          path: '/srv/main.ts',
          content: 'const value = 2;',
        },
      });
      expect(useFileWorkspaceStore.getState().tabs[0].dirty).toBe(false);
    });
  });

  it('preserves edits made in a remounted pane while an earlier save completes', async () => {
    let finishSave!: (value: { success: true; data: null }) => void;
    const pendingSave = new Promise<{ success: true; data: null }>((resolve) => { finishSave = resolve; });
    const original = safeInvokeMock.getMockImplementation()!;
    safeInvokeMock.mockImplementation((command: string, ...args: unknown[]) => command === 'sftp_write_file' ? pendingSave : original(command, ...args));
    const first = render(<FileWorkspace tab={tab} isActive />);
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(editor, { target: { value: 'saved revision' } });
    fireEvent.keyDown(editor, { key: 's', ctrlKey: true });
    first.unmount();
    render(<FileWorkspace tab={useFileWorkspaceStore.getState().tabs[0]} isActive />);
    const movedEditor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(movedEditor, { target: { value: 'newer unsaved revision' } });
    await act(async () => { finishSave({ success: true, data: null }); await pendingSave; });
    expect(movedEditor).toHaveValue('newer unsaved revision');
    expect(readTextBuffer(tab.id)).toEqual(expect.objectContaining({ text: 'newer unsaved revision', saved: 'saved revision' }));
    expect(useFileWorkspaceStore.getState().tabs[0].dirty).toBe(true);
  });

  it('keeps the tab dirty when editing continues during an in-flight save', async () => {
    let finishSave: ((value: { success: true; data: null }) => void) | undefined;
    const pendingSave = new Promise<{ success: true; data: null }>((resolve) => {
      finishSave = resolve;
    });
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_read_file') {
        return Promise.resolve({
          success: true,
          data: {
            content: 'const value = 1;',
            isBinary: false,
            size: 16,
            truncated: false,
            mimeType: 'text/typescript',
          },
        });
      }
      if (command === 'sftp_write_file') return pendingSave;
      return Promise.resolve({ success: true, data: null });
    });

    render(<FileWorkspace tab={tab} isActive />);
    const editor = await screen.findByRole('textbox', { name: 'File editor' });
    fireEvent.change(editor, { target: { value: 'const value = 2;' } });
    fireEvent.keyDown(editor, { key: 's', ctrlKey: true });
    fireEvent.change(editor, { target: { value: 'const value = 3;' } });

    await waitFor(() => expect(safeInvokeMock).toHaveBeenCalledWith(
      'sftp_write_file',
      expect.objectContaining({ request: expect.objectContaining({ content: 'const value = 2;' }) })
    ));
    await act(async () => {
      finishSave?.({ success: true, data: null });
      await pendingSave;
    });
    expect(useFileWorkspaceStore.getState().tabs[0].dirty).toBe(true);
  });
});
