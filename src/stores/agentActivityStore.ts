import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';

export interface AgentActivity {
  sequence: number;
  id: string;
  tool: string;
  summary: string;
  status: 'started' | 'succeeded' | 'failed';
  sessionId?: string | null;
  timestamp: number;
}

export function mergeAgentActivity(current: AgentActivity[], incoming: AgentActivity[]): AgentActivity[] {
  const byId = new Map(current.map((item) => [item.id, item]));
  for (const item of incoming) {
    const previous = byId.get(item.id);
    if (!previous || item.sequence > previous.sequence) byId.set(item.id, item);
  }
  return [...byId.values()].sort((a, b) => b.sequence - a.sequence);
}

interface ActivityState {
  activities: AgentActivity[];
  cursor: number | null;
  olderCursor: number | null;
  hasOlder: boolean;
  refreshing: boolean;
  loadingOlder: boolean;
  error: string | null;
  refresh: () => Promise<boolean>;
  loadOlder: () => Promise<void>;
}

export const useAgentActivityStore = create<ActivityState>((set, get) => ({
  activities: [], cursor: null, olderCursor: null, hasOlder: false,
  refreshing: false, loadingOlder: false, error: null,
  refresh: async () => {
    if (get().refreshing) return false;
    const cursor = get().cursor;
    set({ refreshing: true });
    try {
      const result = await safeInvoke<AgentActivity[]>('agent_activity_list', { after: cursor, before: null, limit: 200 });
      if (!result.success) {
        set({ error: result.error.message });
        return false;
      }
      const events = result.data;
      set((state) => ({
        activities: mergeAgentActivity(state.activities, events),
        cursor: Math.max(cursor ?? 0, ...events.map((event) => event.sequence)),
        ...(cursor === null ? {
          olderCursor: events.length ? Math.min(...events.map((event) => event.sequence)) : null,
          hasOlder: events.length === 200,
        } : {}),
        error: null,
      }));
      return events.some((event) => event.status === 'succeeded'
        && /(?:session_create|session_kill)$/.test(event.tool));
    } catch (error) {
      set({ error: error instanceof Error ? error.message : 'Could not read Agent activity' });
      return false;
    } finally { set({ refreshing: false }); }
  },
  loadOlder: async () => {
    if (get().loadingOlder || !get().hasOlder || get().olderCursor === null) return;
    const before = get().olderCursor;
    set({ loadingOlder: true });
    try {
      const result = await safeInvoke<AgentActivity[]>('agent_activity_list', { after: null, before, limit: 200 });
      if (!result.success) { set({ error: result.error.message }); return; }
      set((state) => ({
        activities: mergeAgentActivity(state.activities, result.data),
        olderCursor: result.data.length ? Math.min(...result.data.map((event) => event.sequence)) : before,
        hasOlder: result.data.length === 200,
        error: null,
      }));
    } catch (error) { set({ error: error instanceof Error ? error.message : 'Could not read history' }); }
    finally { set({ loadingOlder: false }); }
  },
}));
