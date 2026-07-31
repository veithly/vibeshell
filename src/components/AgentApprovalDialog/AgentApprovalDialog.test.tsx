import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useAgentApprovalStore } from '../../stores/agentApprovalStore';
import { AgentApprovalDialog } from './AgentApprovalDialog';

const mocks = vi.hoisted(() => ({
  safeInvoke: vi.fn(async () => ({ success: true, data: undefined })),
}));

vi.mock('../../lib/tauri', () => ({ safeInvoke: mocks.safeInvoke }));
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, values?: { count?: number }) => (
      values?.count === undefined ? key : `${key}:${values.count}`
    ),
  }),
}));

describe('AgentApprovalDialog', () => {
  beforeEach(() => {
    mocks.safeInvoke.mockClear();
    useAgentApprovalStore.setState({
      queue: [{
        id: 'approval-1',
        tool: 'session_send_input',
        command: 'rm -rf /tmp/example',
        reasons: ['Recursive removal'],
        sessionId: 'session-1',
        timestamp: 1,
      }],
      autoApproveUntil: null,
      initialized: true,
      resolvingIds: [],
      error: null,
    });
  });

  afterEach(() => cleanup());

  it('shows the risky command and offers the fixed five-hour action', async () => {
    render(<AgentApprovalDialog />);

    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
    expect(screen.getByText('rm -rf /tmp/example')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'agentApproval.approveAuto' }));

    await waitFor(() => {
      expect(mocks.safeInvoke).toHaveBeenCalledWith('resolve_agent_approval', {
        request: { id: 'approval-1', approved: true, autoApproveHours: 5 },
      });
    });
  });
});
