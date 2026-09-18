import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const { safeInvoke, success, notifyError } = vi.hoisted(() => ({ safeInvoke: vi.fn(), success: vi.fn(), notifyError: vi.fn() }));
vi.mock('../../lib/tauri', () => ({ safeInvoke }));
vi.mock('../../stores/notificationStore', () => ({
  useNotificationStore: Object.assign(() => ({ success, error: notifyError }), { getState: () => ({ success, error: notifyError }) }),
}));
import { useServerStore, type Server } from '../../stores/serverStore';
import { EditServerDialog } from './EditServerDialog';
const server: Server = { id: 'server', name: 'Example', host: 'test.invalid', port: 22, username: 'test',
  auth_type: 'password', tags: [], created_at: 1, updated_at: 1 };
afterEach(cleanup);
beforeEach(() => {
  success.mockReset(); notifyError.mockReset(); safeInvoke.mockReset();
  useServerStore.setState({ servers: [server], groups: [], loading: false, error: null });
  safeInvoke.mockImplementation(async (command: string) => {
    if (command === 'get_servers') return { success: true, data: [server] };
    if (command === 'get_groups') return { success: true, data: [] };
    if (command === 'update_server') return { success: true, data: undefined };
    return { success: false, error: { message: `Unexpected test command: ${command}` } };
  });
});
describe('atomic saved credential editing', () => {
  it('keeps omitted credentials and never fetches the saved password into the editor', async () => {
    const close = vi.fn();
    const { container } = render(<EditServerDialog isOpen server={server} onClose={close} />);
    fireEvent.change(screen.getByDisplayValue('Example'), { target: { value: 'Renamed' } });
    fireEvent.submit(container.querySelector('form')!);
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    const update = safeInvoke.mock.calls.find(([command]) => command === 'update_server')![1].updates;
    expect(update.name).toBe('Renamed'); expect(update.credentials).toBeUndefined();
    expect(safeInvoke.mock.calls.some(([command]) => command === 'get_credential')).toBe(false);
  });
  it('saves a replacement password but never puts it into the shared server cache', async () => {
    const close = vi.fn();
    const { container } = render(<EditServerDialog isOpen server={server} onClose={close} />);
    fireEvent.change(screen.getByLabelText('New password'), { target: { value: 'temporary-new-secret' } });
    fireEvent.submit(container.querySelector('form')!);
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    const update = safeInvoke.mock.calls.find(([command]) => command === 'update_server')![1].updates;
    expect(update.credentials).toEqual({ credential: 'temporary-new-secret' });
    expect(JSON.stringify(useServerStore.getState().servers)).not.toContain('temporary-new-secret');
  });
  it('does not close or report success on a rejected save, and preserves the draft', async () => {
    safeInvoke.mockResolvedValue({ success: false, error: { message: 'Database is busy', isTauriUnavailable: false } });
    const close = vi.fn();
    const { container } = render(<EditServerDialog isOpen server={server} onClose={close} />);
    fireEvent.change(screen.getByLabelText('New password'), { target: { value: 'unsaved-secret' } });
    fireEvent.submit(container.querySelector('form')!);
    await waitFor(() => expect(screen.getByText('Database is busy')).toBeInTheDocument());
    expect(close).not.toHaveBeenCalled(); expect(success).not.toHaveBeenCalled();
    expect(screen.getByLabelText('New password')).toHaveValue('unsaved-secret');
    expect(useServerStore.getState().servers[0].name).toBe('Example');
  });
  it('can explicitly clear a passphrase without overwriting the saved private key', async () => {
    const close = vi.fn();
    const { container } = render(<EditServerDialog isOpen server={{ ...server, auth_type: 'key' }} onClose={close} />);
    fireEvent.click(screen.getByLabelText('Change key passphrase'));
    fireEvent.submit(container.querySelector('form')!);
    await waitFor(() => expect(close).toHaveBeenCalledOnce());
    const update = safeInvoke.mock.calls.find(([command]) => command === 'update_server')![1].updates;
    expect(update.credentials).toEqual({ passphrase: '' });
    expect(update.auth_type).toBe('key_with_passphrase');
  });
  it('clears secret drafts when the dialog closes and reopens', () => {
    const { rerender } = render(<EditServerDialog isOpen server={server} onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText('New password'), { target: { value: 'discard-me' } });
    rerender(<EditServerDialog isOpen={false} server={server} onClose={() => {}} />);
    rerender(<EditServerDialog isOpen server={server} onClose={() => {}} />);
    expect(screen.getByLabelText('New password')).toHaveValue('');
  });
});
