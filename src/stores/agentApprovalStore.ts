import { create } from 'zustand';
import { safeInvoke } from '../lib/tauri';

/**
 * A pending approval request emitted by the backend Agent Gateway when the AI
 * tries to run a command classified as dangerous.
 */
export interface AgentApprovalRequest {
  id: string;
  sequence?: number;
  tool: string;
  command: string;
  reasons: string[];
  sessionId: string | null;
  timestamp: number;
}

/** Hours the "approve and auto-confirm" button opens the auto-approve window. */
export const AUTO_APPROVE_HOURS = 5;

interface AgentApprovalStatus {
  autoApproveUntil: number | null;
  pending: AgentApprovalRequest[];
}

const mergePendingRequests = (
  current: AgentApprovalRequest[],
  incoming: AgentApprovalRequest[],
  resolvedIds: Set<string> = new Set()
) => {
  const merged = new Map(current.map((request) => [request.id, request]));
  for (const request of incoming) {
    if (!resolvedIds.has(request.id)) {
      merged.set(request.id, request);
    }
  }
  return [...merged.values()].sort(
    (a, b) => a.timestamp - b.timestamp || (a.sequence ?? 0) - (b.sequence ?? 0)
  );
};

interface AgentApprovalState {
  /** FIFO queue of requests awaiting a decision; the dialog shows the head. */
  queue: AgentApprovalRequest[];
  /** Epoch ms until which dangerous commands auto-approve, or null. */
  autoApproveUntil: number | null;
  /** Guard so event listeners are only wired once. */
  initialized: boolean;
  /** Request ids currently being resolved through Tauri. */
  resolvingIds: string[];
  /** Last resolution error, kept visible so a failed decision is not lost. */
  error: string | null;

  /** Subscribe to gateway approval events and load the initial state. */
  initialize: () => Promise<void>;
  /** Approve the request just this once. */
  approveOnce: (id: string) => Promise<void>;
  /** Deny the request; the agent receives an error. */
  deny: (id: string) => Promise<void>;
  /** Approve and open a 5-hour auto-approve window. */
  approveWithAutoConfirm: (id: string) => Promise<void>;
  /** Close the auto-approve window immediately. */
  cancelAutoApprove: () => Promise<void>;
}

export const useAgentApprovalStore = create<AgentApprovalState>((set, get) => {
  const resolve = async (
    id: string,
    request: { id: string; approved: boolean; autoApproveHours?: number }
  ) => {
    set((state) => ({
      resolvingIds: state.resolvingIds.includes(id)
        ? state.resolvingIds
        : [...state.resolvingIds, id],
      error: null,
    }));

    const result = await safeInvoke('resolve_agent_approval', { request });
    if (result.success) {
      set((state) => ({
        queue: state.queue.filter((item) => item.id !== id),
        resolvingIds: state.resolvingIds.filter((item) => item !== id),
      }));
    } else {
      set((state) => ({
        resolvingIds: state.resolvingIds.filter((item) => item !== id),
        error: result.error.message,
      }));
    }
  };

  return {
    queue: [],
    autoApproveUntil: null,
    initialized: false,
    resolvingIds: [],
    error: null,

  initialize: async () => {
    if (get().initialized) return;
    set({ initialized: true });

    // Track resolutions observed while the backend snapshot is in flight so a
    // stale snapshot cannot resurrect a request that just finished.
    const resolvedDuringInitialization = new Set<string>();

    try {
      const { listen } = await import('@tauri-apps/api/event');

      await Promise.all([
        listen<AgentApprovalRequest>('agent-approval-request', (event) => {
          const request = event.payload;
          set((state) => ({
            queue: mergePendingRequests(state.queue, [request]),
          }));
        }),
        listen<{ id: string }>('agent-approval-resolved', (event) => {
          const id = event.payload?.id;
          if (!id) return;
          resolvedDuringInitialization.add(id);
          set((state) => ({
            queue: state.queue.filter((item) => item.id !== id),
            resolvingIds: state.resolvingIds.filter((item) => item !== id),
          }));
        }),
        listen<{ autoApproveUntil: number | null }>(
          'agent-approval-state',
          (event) => {
            set({ autoApproveUntil: event.payload?.autoApproveUntil ?? null });
          }
        ),
      ]);
    } catch {
      // Browser-only previews do not have Tauri's event bridge.
    }

    // Fetch after all listeners are active. The pending snapshot recovers any
    // requests emitted before the frontend event bridge finished initializing.
    const status = await safeInvoke<AgentApprovalStatus>('get_agent_guard_status');
    if (status.success) {
      set((state) => ({
        autoApproveUntil: status.data.autoApproveUntil ?? null,
        queue: mergePendingRequests(
          state.queue,
          status.data.pending ?? [],
          resolvedDuringInitialization
        ),
      }));
    }
  },

  approveOnce: async (id: string) => {
    await resolve(id, { id, approved: true });
  },

  deny: async (id: string) => {
    await resolve(id, { id, approved: false });
  },

  approveWithAutoConfirm: async (id: string) => {
    await resolve(id, {
      id,
      approved: true,
      autoApproveHours: AUTO_APPROVE_HOURS,
    });
  },

  cancelAutoApprove: async () => {
    const result = await safeInvoke('cancel_agent_auto_approve');
    if (result.success) {
      set({ autoApproveUntil: null, error: null });
    } else {
      set({ error: result.error.message });
    }
  },
  };
});
