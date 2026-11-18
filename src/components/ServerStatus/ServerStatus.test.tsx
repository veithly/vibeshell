import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ServerStatus } from './ServerStatus';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('../../lib/tauri', () => ({safeInvoke: invoke}));
vi.mock('react-i18next', () => ({useTranslation: () => ({t: (key: string) => key})}));
afterEach(() => {cleanup(); vi.clearAllMocks();});

describe('bottom performance panel', () => {
  it('fetches once in manual mode and does not immediately retry failures', async () => {
    invoke.mockResolvedValue({success: false, error: {message: 'offline'}});
    render(<ServerStatus sessionId="test-session" defaultCollapsed={false} defaultRefreshInterval="manual" />);
    await screen.findByText('offline');
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 40)); });
    expect(invoke).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTitle('Refresh now'));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
  });
  it('has a keyboard-adjustable separator and a close action independent of its terminal', async () => {
    invoke.mockResolvedValue({success: false, error: {message: 'offline'}});
    const close = vi.fn();
    render(<ServerStatus sessionId="test-session" defaultCollapsed={false} defaultHeight={230} onClose={close} />);
    await screen.findByText('offline');
    const divider = screen.getByRole('separator');
    fireEvent.keyDown(divider, {key: 'ArrowUp'});
    expect(divider).toHaveAttribute('aria-valuenow', '250');
    fireEvent.click(screen.getByRole('button', {name: 'sidebar.closePerformance'}));
    expect(close).toHaveBeenCalledOnce();
  });
});
