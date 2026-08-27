import { useCallback, useRef, useState } from 'react';
import { safeInvoke } from '../../lib/tauri';
import type {
  PluginAction,
  PluginExecutionResult,
  PluginInputValues,
  PluginRecord,
} from '../../plugins/types';

export interface DashboardActionOutcome {
  output: string;
  durationMs: number;
  truncated: boolean;
}

/**
 * Runs plugin actions for a dashboard without touching the plugin store's
 * global operation state, so background refreshes never flicker unrelated
 * Run buttons. Errors stay local to the calling dashboard.
 */
export function usePluginAction(pluginId: string, sessionId: string) {
  const [runningAction, setRunningAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sequenceRef = useRef(0);

  const run = useCallback(
    async (actionId: string, inputs?: PluginInputValues): Promise<DashboardActionOutcome | null> => {
      const sequence = ++sequenceRef.current;
      setError(null);
      setRunningAction(actionId);
      try {
        const result = await safeInvoke<PluginExecutionResult>('plugin_execute', {
          request: {
            pluginId,
            actionId,
            sessionId,
            inputs: inputs ?? {},
            trySudo: false,
          },
        });
        if (sequence !== sequenceRef.current) return null;
        if (result.success) {
          return result.data;
        }
        setError(result.error.message);
        return null;
      } catch (caught) {
        if (sequence !== sequenceRef.current) return null;
        setError(caught instanceof Error ? caught.message : String(caught));
        return null;
      } finally {
        if (sequence === sequenceRef.current) {
          setRunningAction(null);
        }
      }
    },
    [pluginId, sessionId]
  );

  const clearError = useCallback(() => setError(null), []);

  return { run, runningAction, error, clearError };
}

export function findAction(plugin: PluginRecord, actionId: string): PluginAction | undefined {
  if (plugin.manifest.entry.type !== 'commands') return undefined;
  return plugin.manifest.entry.actions.find((action) => action.id === actionId);
}
