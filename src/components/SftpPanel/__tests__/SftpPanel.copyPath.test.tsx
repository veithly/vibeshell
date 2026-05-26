import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { SftpPanel } from '../SftpPanel';

const safeInvokeMock = vi.fn();
const writeTextMock = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: (...args: unknown[]) => safeInvokeMock(...args),
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe('SftpPanel path copy', () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    writeTextMock.mockReset();
    writeTextMock.mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextMock },
      configurable: true,
    });

    safeInvokeMock.mockReset();
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') {
        return Promise.resolve({ success: true, data: true });
      }
      if (command === 'sftp_pwd') {
        return Promise.resolve({ success: true, data: '/var/www' });
      }
      if (command === 'sftp_list_dir') {
        return Promise.resolve({
          success: true,
          data: [
            {
              name: 'app.log',
              path: '/var/www/app.log',
              isDirectory: false,
              size: 42,
              modifiedAt: 1_700_000_000,
              permissions: '-rw-r--r--',
            },
          ],
        });
      }
      return Promise.resolve({ success: true, data: null });
    });
  });

  it('copies a selected entry path from the context menu', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} />);

    const entry = await screen.findByText('app.log');
    fireEvent.contextMenu(entry);

    fireEvent.click(await screen.findByText('Copy Path'));

    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith('/var/www/app.log');
    });
  });

  it('copies the current directory path from the toolbar when nothing is selected', async () => {
    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} />);

    await screen.findByText('app.log');
    fireEvent.click(screen.getByTitle('Copy current path'));

    await waitFor(() => {
      expect(writeTextMock).toHaveBeenCalledWith('/var/www');
    });
  });
});
