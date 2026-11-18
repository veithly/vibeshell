import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { safeInvoke, addSession } = vi.hoisted(() => ({ safeInvoke: vi.fn(), addSession: vi.fn() }));
vi.mock('../lib/tauri', () => ({ safeInvoke }));
vi.mock('../stores/sessionStore', () => ({ useSessionStore: { getState: () => ({ addSession }) } }));
import { ContainerPanel } from './ContainerPanel';
const id = 'a'.repeat(64);
afterEach(cleanup);
beforeEach(() => {
  safeInvoke.mockReset(); addSession.mockReset();
  safeInvoke.mockImplementation(async (command: string) => command === 'container_list'
    ? { success: true, data: [{ ID: id, Names: 'alpha', Image: 'busybox', State: 'running', Status: 'Up' }, { ID: 'b'.repeat(64), Names: 'stopped-node', Image: 'busybox', State: 'exited', Status: 'Exited' }] }
    : { success: true, data: { id: 'child', server_id: 'host-profile', server_name: 'host / alpha', state: 'connected', created_at: 1, container: { parent_session_id: 'parent', container_id: id, name: 'alpha', shell: '/bin/sh' } } });
});
it('opens a full-ID child and activates it without offering to start stopped nodes', async () => {
  const connected = vi.fn(), close = vi.fn();
  render(<ContainerPanel parentId="parent" onConnected={connected} onClose={close} />);
  fireEvent.click(await screen.findByRole('button', { name: /alpha/ }));
  await waitFor(() => expect(connected).toHaveBeenCalledWith('child'));
  expect(safeInvoke).toHaveBeenCalledWith('container_open', { request: { parent_session_id: 'parent', container: id, user: null, workdir: null, shell: '/bin/sh' } });
  expect(addSession).toHaveBeenCalledWith(expect.objectContaining({ container: expect.objectContaining({ container_id: id }) }));
  expect(screen.queryByRole('button', { name: /stopped-node/ })).not.toBeInTheDocument();
  expect(close).toHaveBeenCalledOnce();
});
it('keeps the panel open and surfaces connection errors without creating a host tab', async () => {
  const connected = vi.fn(), close = vi.fn();
  render(<ContainerPanel parentId="parent" onConnected={connected} onClose={close} />);
  const button = await screen.findByRole('button', { name: /alpha/ });
  safeInvoke.mockResolvedValue({ success: false, error: { message: 'Container stopped before startup' } });
  fireEvent.click(button);
  expect(await screen.findByRole('alert')).toHaveTextContent('Container stopped');
  expect(connected).not.toHaveBeenCalled(); expect(addSession).not.toHaveBeenCalled(); expect(close).not.toHaveBeenCalled();
});
