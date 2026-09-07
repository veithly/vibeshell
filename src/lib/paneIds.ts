/**
 * Mosaic pane leaf ids. The terminal mosaic stores plain strings, so plugin
 * panes and session panes share one id space distinguished by a prefix.
 */
export const SESSION_PANE_PREFIX = 'session:';
export const PLUGIN_PANE_PREFIX = 'plugin:';
export const FILE_PANE_PREFIX = 'file:';

export function filePaneId(tabId: string): string {
  return `${FILE_PANE_PREFIX}${encodeURIComponent(tabId)}`;
}

/** MIME type used when dragging a plugin tab from the session tab strip. */
export const PLUGIN_TAB_DND_MIME = 'application/x-vibeshell-plugin-tab';

export function sessionPaneId(sessionId: string): string {
  return `${SESSION_PANE_PREFIX}${sessionId}`;
}

export function pluginPaneId(tabId: string): string {
  return `${PLUGIN_PANE_PREFIX}${tabId}`;
}

export type PaneId =
  | { kind: 'session'; id: string }
  | { kind: 'plugin'; id: string }
  | { kind: 'file'; id: string }
  | { kind: 'unknown'; id: string };

export function parsePaneId(leaf: string): PaneId {
  if (leaf.startsWith(SESSION_PANE_PREFIX)) {
    return { kind: 'session', id: leaf.slice(SESSION_PANE_PREFIX.length) };
  }
  if (leaf.startsWith(PLUGIN_PANE_PREFIX)) {
    return { kind: 'plugin', id: leaf.slice(PLUGIN_PANE_PREFIX.length) };
  }
  if (leaf.startsWith(FILE_PANE_PREFIX)) {
    try {
      return { kind: 'file', id: decodeURIComponent(leaf.slice(FILE_PANE_PREFIX.length)) };
    } catch {
      return { kind: 'unknown', id: leaf };
    }
  }
  return { kind: 'unknown', id: leaf };
}
