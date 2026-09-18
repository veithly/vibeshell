import { beforeEach, describe, expect, it, vi } from 'vitest';
const safeInvoke = vi.hoisted(() => vi.fn());
vi.mock('../lib/tauri', () => ({ safeInvoke }));
import { mergeAgentActivity, useAgentActivityStore, type AgentActivity } from './agentActivityStore';
const event = (sequence: number, id = `run-${sequence}`, status: AgentActivity['status'] = 'succeeded'): AgentActivity => ({
  sequence, id, tool: 'cli.exec', summary: 'printf repeated', status, sessionId: 's', timestamp: sequence,
});
beforeEach(() => {
  safeInvoke.mockReset();
  useAgentActivityStore.setState({ activities: [], cursor: null, olderCursor: null, hasOlder: false,
    refreshing: false, loadingOlder: false, error: null });
});
describe('durable Agent activity', () => {
  it('keeps repeated commands and never overwrites completion with an older start', () => {
    const merged = mergeAgentActivity([event(3, 'a'), event(4, 'b')], [event(1, 'a', 'started'), event(2, 'b', 'started')]);
    expect(merged.map((item) => item.id)).toEqual(['b', 'a']);
    expect(merged.every((item) => item.status === 'succeeded')).toBe(true);
  });
  it('advances through full incremental pages without skipping events', async () => {
    safeInvoke.mockResolvedValueOnce({ success: true, data: [] });
    await useAgentActivityStore.getState().refresh();
    safeInvoke.mockResolvedValueOnce({ success: true, data: Array.from({ length: 200 }, (_, index) => event(index + 1)) });
    await useAgentActivityStore.getState().refresh();
    safeInvoke.mockResolvedValueOnce({ success: true, data: [event(201)] });
    await useAgentActivityStore.getState().refresh();
    expect(safeInvoke).toHaveBeenLastCalledWith('agent_activity_list', { after: 200, before: null, limit: 200 });
    expect(useAgentActivityStore.getState().activities).toHaveLength(201);
    expect(useAgentActivityStore.getState().cursor).toBe(201);
  });
  it('loads older history without moving the live cursor backwards', async () => {
    safeInvoke.mockResolvedValueOnce({ success: true, data: Array.from({ length: 200 }, (_, index) => event(400 - index)) });
    await useAgentActivityStore.getState().refresh();
    safeInvoke.mockResolvedValueOnce({ success: true, data: [event(200), event(199)] });
    await useAgentActivityStore.getState().loadOlder();
    expect(safeInvoke).toHaveBeenLastCalledWith('agent_activity_list', { after: null, before: 201, limit: 200 });
    expect(useAgentActivityStore.getState().cursor).toBe(400);
    expect(useAgentActivityStore.getState().activities).toHaveLength(202);
  });
  it('preserves history and cursor when native reads fail', async () => {
    useAgentActivityStore.setState({ activities: [event(9)], cursor: 9 });
    safeInvoke.mockResolvedValue({ success: false, error: { message: 'database unavailable' } });
    await useAgentActivityStore.getState().refresh();
    expect(useAgentActivityStore.getState().activities).toEqual([event(9)]);
    expect(useAgentActivityStore.getState().cursor).toBe(9);
    expect(useAgentActivityStore.getState().error).toBe('database unavailable');
    expect(useAgentActivityStore.getState().refreshing).toBe(false);
  });
  it('notifies the workspace when an Agent creates a session', async () => {
    safeInvoke.mockResolvedValue({ success: true, data: [{ ...event(1), tool: 'cli.session_create' }] });
    expect(await useAgentActivityStore.getState().refresh()).toBe(true);
  });
});
