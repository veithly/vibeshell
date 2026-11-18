import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { safeInvoke } = vi.hoisted(() => ({ safeInvoke: vi.fn() }));
vi.mock('../../lib/tauri', () => ({ safeInvoke }));
import { NetworkProxyControl } from './NetworkProxyControl';
const config = { mode: 'custom', url: 'socks5h://proxy.invalid:1080', username: 'user', hasPassword: true };
afterEach(cleanup);
beforeEach(() => { safeInvoke.mockReset(); safeInvoke.mockResolvedValue({ success: true, data: config }); });
it('never loads a stored password into the form and preserves it when untouched', async () => {
  render(<NetworkProxyControl />);
  await waitFor(() => expect(screen.getByLabelText('Proxy password')).toBeEnabled());
  expect(screen.getByLabelText('Proxy password')).toHaveValue('');
  fireEvent.click(screen.getByText('Save network route'));
  await waitFor(() => expect(safeInvoke).toHaveBeenCalledWith('set_network_proxy', { settings: { mode: 'custom', url: config.url, username: 'user', password: null } }));
});
it('preserves password whitespace and clears the draft after a successful save', async () => {
  render(<NetworkProxyControl />);
  await waitFor(() => expect(screen.getByLabelText('Proxy password')).toBeEnabled());
  fireEvent.change(screen.getByLabelText('Proxy password'), { target: { value: ' padded secret ' } });
  fireEvent.click(screen.getByText('Save network route'));
  await screen.findByRole('status');
  expect(safeInvoke).toHaveBeenCalledWith('set_network_proxy', { settings: { mode: 'custom', url: config.url, username: 'user', password: ' padded secret ' } });
  expect(screen.getByLabelText('Proxy password')).toHaveValue('');
});
it('explicitly clears saved credentials and displays rejected save errors', async () => {
  render(<NetworkProxyControl />);
  await waitFor(() => expect(screen.getByLabelText('Clear stored proxy password')).toBeEnabled());
  fireEvent.click(screen.getByLabelText('Clear stored proxy password'));
  safeInvoke.mockResolvedValue({ success: false, error: { message: 'Invalid proxy URL' } });
  fireEvent.click(screen.getByText('Save network route'));
  expect(await screen.findByRole('alert')).toHaveTextContent('Invalid proxy URL');
  expect(safeInvoke).toHaveBeenCalledWith('set_network_proxy', { settings: { mode: 'custom', url: config.url, username: 'user', password: '' } });
});
