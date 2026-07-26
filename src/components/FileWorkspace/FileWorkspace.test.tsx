import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { FileWorkspace } from './FileWorkspace';
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

  afterEach(() => cleanup());

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
