import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { SftpPanel, type SftpEntry } from '../SftpPanel';
import { useRuntimeCapabilitiesStore } from '../../../stores/runtimeCapabilitiesStore';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useFileWorkspaceStore } from '../../../stores/fileWorkspaceStore';

const safeInvokeMock = vi.fn();

vi.mock('../../../lib/tauri', () => ({
  safeInvoke: (...args: unknown[]) => safeInvokeMock(...args),
}));

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: vi.fn().mockResolvedValue(() => {}),
  }),
}));

function entry(name: string, isDirectory = false): SftpEntry {
  return {
    name,
    path: `/home/test/${name}`,
    isDirectory,
    size: isDirectory ? 0 : 128,
    modifiedAt: 1_700_000_000,
    permissions: isDirectory ? 'drwxr-xr-x' : '-rw-r--r--',
  };
}

describe('SftpPanel interactions', () => {
  beforeEach(() => {
    useRuntimeCapabilitiesStore.setState({
      status: 'ready',
      capabilities: {
        platform: 'linux',
        isMobile: false,
        windowControls: true,
        localShell: true,
        agentGateway: true,
        desktopUpdater: true,
        cliIpc: true,
        directoryTransfer: true,
        backgroundTunnels: true,
      },
    });
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: query === '(pointer: coarse)',
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });
    safeInvokeMock.mockReset();
    useNotificationStore.getState().clearAll();
    useFileWorkspaceStore.setState({ tabs: [], activeTabId: null });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('coalesces repeated activation of the same directory while it is loading', async () => {
    let resolveDirectory: ((value: { success: true; data: SftpEntry[] }) => void) | undefined;
    const pendingDirectory = new Promise<{ success: true; data: SftpEntry[] }>((resolve) => {
      resolveDirectory = resolve;
    });

    safeInvokeMock.mockImplementation((command: string, args?: { request?: { path?: string } }) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir' && args?.request?.path === '/home/test/src') {
        return pendingDirectory;
      }
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('src', true), entry('notes.txt')] });
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    const directory = await screen.findByText('src');
    fireEvent.click(directory);
    fireEvent.click(directory);
    fireEvent.doubleClick(directory);

    const directoryRequests = () => safeInvokeMock.mock.calls.filter(
      ([command, args]) => command === 'sftp_list_dir'
        && (args as { request?: { path?: string } })?.request?.path === '/home/test/src'
    );
    expect(directoryRequests()).toHaveLength(1);

    resolveDirectory?.({ success: true, data: [entry('index.ts')] });
    expect(await screen.findByText('index.ts')).toBeInTheDocument();
  });

  it('opens text, PDF, media, and archive files in deduplicated workspace tabs', async () => {
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        matches: false,
        media: query,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      })),
    });
    const files = [entry('notes.ts'), entry('manual.pdf'), entry('demo.mp4'), entry('archive.zip')];
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') return Promise.resolve({ success: true, data: files });
      return Promise.resolve({ success: true, data: null });
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    for (const filename of ['notes.ts', 'manual.pdf', 'demo.mp4', 'archive.zip']) {
      fireEvent.doubleClick(await screen.findByText(filename));
    }
    fireEvent.doubleClick(screen.getByText('archive.zip'));

    const state = useFileWorkspaceStore.getState();
    expect(state.tabs.map((tab) => [tab.name, tab.kind])).toEqual([
      ['notes.ts', 'text'],
      ['manual.pdf', 'pdf'],
      ['demo.mp4', 'video'],
      ['archive.zip', 'archive'],
    ]);
    expect(state.tabs).toHaveLength(4);
    expect(state.tabs.find((tab) => tab.id === state.activeTabId)?.name).toBe('archive.zip');
    expect(safeInvokeMock).not.toHaveBeenCalledWith('sftp_read_file', expect.anything());
  });

  it('keeps a long file listing inside a shrinkable scroll region', async () => {
    const files = Array.from({ length: 80 }, (_, index) => entry(`file-${index}.txt`));
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') return Promise.resolve({ success: true, data: files });
      return Promise.resolve({ success: true, data: null });
    });

    const { container } = render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" defaultHeight={240} />
    );

    await screen.findByText('file-79.txt');
    const fileList = container.querySelector('.flex-1.overflow-auto.relative');
    expect(fileList).not.toBeNull();
    expect(fileList).toHaveClass('min-h-0', 'pb-6');
    expect(fileList?.parentElement).toHaveClass('min-h-0');
  });

  it('releases resize interaction state when the window loses focus', async () => {
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') return Promise.resolve({ success: true, data: [entry('notes.txt')] });
      return Promise.resolve({ success: true, data: null });
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);
    await screen.findByText('notes.txt');

    fireEvent.mouseDown(screen.getByTitle('Drag to resize'), { clientX: 400 });
    await waitFor(() => expect(document.body.style.userSelect).toBe('none'));

    fireEvent.blur(window);

    await waitFor(() => {
      expect(document.body.style.userSelect).toBe('');
      expect(document.body.style.cursor).toBe('');
    });
  });

  it('keeps a context menu opened near the bottom-right inside the viewport', async () => {
    Object.defineProperty(window, 'innerWidth', { configurable: true, value: 600 });
    Object.defineProperty(window, 'innerHeight', { configurable: true, value: 600 });

    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('archive.zip')] });
      }
      return Promise.resolve({ success: true, data: null });
    });

    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      const isMenu = this.classList.contains('fixed') && this.textContent?.includes('Copy Path');
      return {
        x: isMenu ? 580 : 0,
        y: isMenu ? 580 : 0,
        width: isMenu ? 220 : 0,
        height: isMenu ? 320 : 0,
        top: isMenu ? 580 : 0,
        right: isMenu ? 800 : 0,
        bottom: isMenu ? 900 : 0,
        left: isMenu ? 580 : 0,
        toJSON: () => ({}),
      };
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);

    const file = await screen.findByText('archive.zip');
    fireEvent.contextMenu(file, { clientX: 580, clientY: 580 });
    const menu = (await screen.findByText('Copy Path')).closest('.fixed') as HTMLElement;

    await waitFor(() => {
      expect(Number.parseFloat(menu.style.left)).toBeLessThanOrEqual(372);
      expect(Number.parseFloat(menu.style.top)).toBeLessThanOrEqual(272);
    });
    expect(menu).toHaveClass('overflow-y-auto');
  });

  it('surfaces the backend delete error with the affected filename', async () => {
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('locked.txt')] });
      }
      if (command === 'sftp_delete') {
        return Promise.resolve({ success: false, error: { message: 'Permission denied' } });
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" defaultWidth={720} />
    );

    fireEvent.click(await screen.findByText('locked.txt'));
    fireEvent.click(screen.getByTitle('Delete'));
    fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      const notifications = useNotificationStore.getState().notifications;
      const notification = notifications[notifications.length - 1];
      expect(notification?.title).toBe('Delete Failed');
      expect(notification?.message).toContain('locked.txt');
      expect(notification?.message).toContain('Permission denied');
    });
  });

  it('confirms deletion inside the app before invoking the backend', async () => {
    const nativeConfirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('delete-me.txt')] });
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" defaultWidth={720} />
    );

    fireEvent.click(await screen.findByText('delete-me.txt'));
    fireEvent.click(screen.getByTitle('Delete'));

    const dialog = await screen.findByRole('alertdialog', { name: 'Delete item?' });
    expect(nativeConfirm).not.toHaveBeenCalled();
    expect(safeInvokeMock.mock.calls.some(([command]) => command === 'sftp_delete')).toBe(false);

    fireEvent.click(within(dialog).getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(safeInvokeMock).toHaveBeenCalledWith('sftp_delete', {
        request: {
          sessionId: 'session-1',
          path: '/home/test/delete-me.txt',
          recursive: false,
        },
      });
    });
  });

  it('deletes every selected path and reports partial failures by filename', async () => {
    safeInvokeMock.mockImplementation((command: string, args?: { request?: { path?: string } }) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('one.txt'), entry('locked.txt')] });
      }
      if (command === 'sftp_delete') {
        return Promise.resolve(
          args?.request?.path?.endsWith('locked.txt')
            ? { success: false, error: { message: 'Permission denied' } }
            : { success: true, data: null }
        );
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" defaultWidth={720} />
    );
    fireEvent.click(await screen.findByText('one.txt'));
    fireEvent.click(screen.getByText('locked.txt'), { ctrlKey: true });
    fireEvent.click(screen.getByTitle('Delete'));
    fireEvent.click(within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      const deletes = safeInvokeMock.mock.calls.filter(([command]) => command === 'sftp_delete');
      expect(deletes).toHaveLength(2);
      expect(deletes.map(([, args]) => (args as { request: { path: string } }).request.path).sort()).toEqual([
        '/home/test/locked.txt',
        '/home/test/one.txt',
      ]);
      const notifications = useNotificationStore.getState().notifications;
      expect(notifications[notifications.length - 1]?.message).toContain(
        'locked.txt: Permission denied'
      );
    });
  });

  it('downloads every selected file and exposes aggregate transfer progress', async () => {
    let resolveFirstDownload: ((value: { success: true; data: null }) => void) | undefined;
    const firstDownload = new Promise<{ success: true; data: null }>((resolve) => {
      resolveFirstDownload = resolve;
    });
    let downloadCount = 0;

    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') {
        return Promise.resolve({ success: true, data: [entry('one.txt'), entry('two.txt')] });
      }
      if (command === 'pick_download_directory') {
        return Promise.resolve({ success: true, data: '/tmp/downloads' });
      }
      if (command === 'sftp_download_file') {
        downloadCount += 1;
        return downloadCount === 1
          ? firstDownload
          : Promise.resolve({ success: true, data: null });
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(
      <SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" defaultWidth={720} />
    );

    fireEvent.click(await screen.findByText('one.txt'));
    fireEvent.click(screen.getByText('two.txt'), { ctrlKey: true });
    fireEvent.click(screen.getByTitle('Download 2 files'));

    expect(await screen.findByTestId('sftp-transfer-progress')).toHaveTextContent('Downloading 1 of 2');
    resolveFirstDownload?.({ success: true, data: null });

    await waitFor(() => {
      const downloads = safeInvokeMock.mock.calls.filter(([command]) => command === 'sftp_download_file');
      expect(downloads).toHaveLength(2);
      expect(downloads.map(([, args]) => args)).toEqual([
        { request: { sessionId: 'session-1', remotePath: '/home/test/one.txt', localPath: '/tmp/downloads/one.txt' } },
        { request: { sessionId: 'session-1', remotePath: '/home/test/two.txt', localPath: '/tmp/downloads/two.txt' } },
      ]);
    });
  });

  it('uploads every file returned by the multi-file picker with visible progress', async () => {
    let resolveFirstUpload: ((value: { success: true; data: null }) => void) | undefined;
    const firstUpload = new Promise<{ success: true; data: null }>((resolve) => {
      resolveFirstUpload = resolve;
    });
    let uploadCount = 0;

    safeInvokeMock.mockImplementation((command: string) => {
      if (command === 'sftp_init') return Promise.resolve({ success: true, data: true });
      if (command === 'sftp_pwd') return Promise.resolve({ success: true, data: '/home/test' });
      if (command === 'sftp_list_dir') return Promise.resolve({ success: true, data: [] });
      if (command === 'pick_files_for_upload') {
        return Promise.resolve({ success: true, data: ['/tmp/one.txt', '/tmp/two.txt'] });
      }
      if (command === 'sftp_upload_file') {
        uploadCount += 1;
        return uploadCount === 1
          ? firstUpload
          : Promise.resolve({ success: true, data: null });
      }
      return Promise.resolve({ success: true, data: null });
    });

    render(<SftpPanel sessionId="session-1" defaultCollapsed={false} dock="right" />);
    await screen.findByText('Empty directory');
    fireEvent.click(screen.getByTitle('Upload file'));

    expect(await screen.findByTestId('sftp-transfer-progress')).toHaveTextContent('Uploading 1 of 2');
    resolveFirstUpload?.({ success: true, data: null });

    await waitFor(() => {
      const uploads = safeInvokeMock.mock.calls.filter(([command]) => command === 'sftp_upload_file');
      expect(uploads).toHaveLength(2);
    });
  });
});
