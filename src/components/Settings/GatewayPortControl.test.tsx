import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
const { safeInvoke } = vi.hoisted(() => ({ safeInvoke: vi.fn() }));
vi.mock('../../lib/tauri', () => ({ safeInvoke }));
import { GatewayPortControl } from './GatewayPortControl';
afterEach(cleanup);
beforeEach(() => {
  safeInvoke.mockReset();
  safeInvoke.mockImplementation(async (name: string) => name === 'get_gateway_port'
    ? { success: true, data: { port: 45678 } } : { success: true });
});
it('loads and saves a fixed port without claiming it is already running', async () => {
  render(<GatewayPortControl />);
  await waitFor(() => expect(screen.getByLabelText('Gateway port')).toHaveValue(45678));
  fireEvent.change(screen.getByLabelText('Gateway port'), { target: { value: '12345' } });
  fireEvent.click(screen.getByText('Save port'));
  await waitFor(() => expect(safeInvoke).toHaveBeenCalledWith('set_gateway_port', { settings: { port: 12345 } }));
  expect(await screen.findByRole('status')).toHaveTextContent('Restart VibeShell');
});
it('rejects invalid ports before persisting', async () => {
  render(<GatewayPortControl />);
  await waitFor(() => expect(screen.getByText('Save port')).toBeEnabled());
  fireEvent.change(screen.getByLabelText('Gateway port'), { target: { value: '65536' } });
  fireEvent.click(screen.getByText('Save port'));
  expect(screen.getByRole('alert')).toHaveTextContent('65535');
  expect(safeInvoke).not.toHaveBeenCalledWith('set_gateway_port', expect.anything());
});
