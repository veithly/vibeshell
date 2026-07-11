export const MAX_TERMINAL_PANES = 9;

export type TerminalPaneLayout = 'grid' | 'columns' | 'rows';

export interface TerminalGridTracks {
  columns: number;
  rows: number;
}

/**
 * Select a compact layout that keeps up to nine shells visible in one workspace.
 */
export function getTerminalGridTracks(
  layout: TerminalPaneLayout,
  paneCount: number
): TerminalGridTracks {
  const count = Math.max(1, Math.min(paneCount, MAX_TERMINAL_PANES));

  if (layout === 'columns') return { columns: count, rows: 1 };
  if (layout === 'rows') return { columns: 1, rows: count };

  if (count <= 3) return { columns: count, rows: 1 };
  if (count <= 4) return { columns: 2, rows: 2 };
  if (count <= 6) return { columns: 3, rows: 2 };
  return { columns: 3, rows: 3 };
}

function uniqueValid(ids: string[], validSessionIds: Set<string>): string[] {
  return ids.filter((id, index) => validSessionIds.has(id) && ids.indexOf(id) === index);
}

export function syncTerminalPanes(
  currentPaneIds: string[],
  sessionIds: string[],
  activeSessionId: string | null
): string[] {
  const validSessionIds = new Set(sessionIds);
  const validPanes = uniqueValid(currentPaneIds, validSessionIds).slice(0, MAX_TERMINAL_PANES);
  if (!activeSessionId || !validSessionIds.has(activeSessionId)) return validPanes;
  if (validPanes.length === 0) return [activeSessionId];
  if (validPanes.includes(activeSessionId)) return validPanes;
  return [activeSessionId, ...validPanes.slice(1)];
}

export function addTerminalPane(currentPaneIds: string[], sessionId: string): string[] {
  if (currentPaneIds.includes(sessionId)) return currentPaneIds;
  return [...currentPaneIds, sessionId].slice(0, MAX_TERMINAL_PANES);
}

export function removeTerminalPane(currentPaneIds: string[], sessionId: string): string[] {
  return currentPaneIds.filter((id) => id !== sessionId);
}
