import { beforeEach, describe, expect, it, vi } from 'vitest';
import { AUTO_APPROVE_HOURS, useAgentApprovalStore } from './agentApprovalStore';

const mocks = vi.hoisted(() => ({
  safeInvoke: vi.fn(),
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
  calls: [] as string[],
}));

vi.mock('../lib/tauri', () => ({
  safeInvoke: mocks.safeInvoke,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (eventName: string, handler: (event: { payload: unknown }) => void) => {
    mocks.calls.push(`listen:${eventName}`);
    mocks.listeners.set(eventName, handler);
    return vi.fn();
  }),
}));

const request = {
  id: 'approval-1',
  tool: 'exec',
  command: 'rm -rf /tmp/example',
  reasons: ['dangerous'],
  sessionId: 'session-1',
  timestamp: 1,
};

describe('agentApprovalStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    mocks.calls.length = 0;
    mocks.safeInvoke.mockImplementation(async (command: string) => {
      mocks.calls.push(`invoke:${command}`);
      return { success: true, data: undefined };
    });
    useAgentApprovalStore.setState({
      queue: [],
      autoApproveUntil: null,
      initialized: false,
      resolvingIds: [],
      error: null,
    });
  });

  it('subscribes to request, resolution, and state events', async () => {
    mocks.safeInvoke.mockImplementationOnce(async (command: string) => {
      mocks.calls.push(`invoke:${command}`);
      return {
        success: true,
        data: { autoApproveUntil: 1234, pending: [] },
      };
    });
    await useAgentApprovalStore.getState().initialize();

    expect(useAgentApprovalStore.getState().autoApproveUntil).toBe(1234);
    expect(mocks.calls.indexOf('invoke:get_agent_guard_status')).toBeGreaterThan(
      mocks.calls.indexOf('listen:agent-approval-state')
    );
    mocks.listeners.get('agent-approval-request')?.({ payload: request });
    expect(useAgentApprovalStore.getState().queue).toEqual([request]);

    mocks.listeners.get('agent-approval-resolved')?.({ payload: { id: request.id } });
    expect(useAgentApprovalStore.getState().queue).toEqual([]);

    mocks.listeners.get('agent-approval-state')?.({ payload: { autoApproveUntil: 5678 } });
    expect(useAgentApprovalStore.getState().autoApproveUntil).toBe(5678);
  });

  it('recovers pending requests that predate listener initialization', async () => {
    const earlierRequest = { ...request, id: 'approval-0', timestamp: 0 };
    mocks.safeInvoke.mockResolvedValueOnce({
      success: true,
      data: {
        autoApproveUntil: null,
        pending: [request, earlierRequest, request],
      },
    });

    await useAgentApprovalStore.getState().initialize();

    expect(useAgentApprovalStore.getState().queue).toEqual([earlierRequest, request]);
  });

  it('does not resurrect a request resolved while the status snapshot is in flight', async () => {
    let finishStatus: ((value: unknown) => void) | undefined;
    mocks.safeInvoke.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishStatus = resolve;
        })
    );

    const initialize = useAgentApprovalStore.getState().initialize();
    await vi.waitFor(() => {
      expect(mocks.listeners.has('agent-approval-resolved')).toBe(true);
      expect(finishStatus).toBeTypeOf('function');
    });

    mocks.listeners.get('agent-approval-resolved')?.({ payload: { id: request.id } });
    finishStatus?.({
      success: true,
      data: { autoApproveUntil: null, pending: [request] },
    });
    await initialize;

    expect(useAgentApprovalStore.getState().queue).toEqual([]);
  });

  it('keeps a request visible when resolving it fails', async () => {
    useAgentApprovalStore.setState({ queue: [request] });
    mocks.safeInvoke.mockResolvedValueOnce({
      success: false,
      error: { message: 'native bridge failed' },
    });

    await useAgentApprovalStore.getState().approveOnce(request.id);

    expect(useAgentApprovalStore.getState().queue).toEqual([request]);
    expect(useAgentApprovalStore.getState().error).toBe('native bridge failed');
    expect(useAgentApprovalStore.getState().resolvingIds).toEqual([]);
  });

  it('requests the fixed five-hour auto-confirm window', async () => {
    useAgentApprovalStore.setState({ queue: [request] });

    await useAgentApprovalStore.getState().approveWithAutoConfirm(request.id);

    expect(mocks.safeInvoke).toHaveBeenCalledWith('resolve_agent_approval', {
      request: {
        id: request.id,
        approved: true,
        autoApproveHours: AUTO_APPROVE_HOURS,
      },
    });
    expect(useAgentApprovalStore.getState().queue).toEqual([]);
  });
});
