import { cursorPosition, getCurrentWindow } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

/**
 * Mouse-driven tab drag with native tear-out.
 *
 * Unlike HTML5 drag-and-drop (whose ghost image never leaves the webview and
 * whose coordinates WebKit clamps to the viewport), this controller follows
 * the iTerm/Chrome model:
 *
 * - press a tab, move a few pixels → drag engages; hovering other tabs of the
 *   same kind reorders them live
 * - while the pointer crosses outside the main window, a detached window is
 *   created under the cursor and handed to the OS native drag loop
 *   (`startDragging`), so the new window tracks the mouse exactly like a
 *   normal window move until the button is released
 * - releasing over a terminal pane's edge splits that pane
 */

export type TabKind = 'session' | 'file' | 'plugin';

export interface TabDragStart {
  kind: TabKind;
  id: string;
  /** Live reorder: move the dragged tab before the hovered one. */
  onReorderOver?: (targetId: string) => void;
  /** Drop over a pane edge: split that pane. */
  onPaneDrop?: (paneId: string, direction: 'row' | 'column') => void;
  /** Cursor left the main window: open this tab in its own OS window at the
   * given cursor position. May resolve to the created window label so the
   * native drag loop takes over. */
  onTearOut: (at?: { x: number; y: number }) => unknown;
}

interface ActiveDragState {
  start: TabDragStart;
  startX: number;
  startY: number;
  engaged: boolean;
  outside: boolean;
}

const ENGAGE_THRESHOLD_PX = 6;
const EDGE_MARGIN_RATIO = 0.3;

let active: ActiveDragState | null = null;
let pollTimer: number | null = null;

function setBodyDragging(on: boolean) {
  document.body.classList.toggle('tab-dragging', on);
}

function clearPaneHighlight() {
  document.querySelectorAll('.pane-drop-hover').forEach((element) => {
    element.classList.remove('pane-drop-hover');
  });
}

function paneEdgeUnder(x: number, y: number): { paneId: string; direction: 'row' | 'column' } | null {
  const element = document.elementFromPoint(x, y);
  const pane = element?.closest?.('[data-pane-id]') as HTMLElement | null;
  if (!pane) return null;
  const paneId = pane.dataset.paneId;
  if (!paneId) return null;

  const rect = pane.getBoundingClientRect();
  const relX = (x - rect.left) / rect.width;
  const relY = (y - rect.top) / rect.height;
  const margin = EDGE_MARGIN_RATIO;
  const rowDepth = margin - Math.min(relX, 1 - relX);
  const columnDepth = margin - Math.min(relY, 1 - relY);
  if (rowDepth < 0 && columnDepth < 0) return null;
  const direction = rowDepth >= columnDepth ? 'row' : 'column';
  return { paneId, direction };
}

function reorderTargetUnder(x: number, y: number, kind: TabKind): string | null {
  const element = document.elementFromPoint(x, y);
  const tab = element?.closest?.('[data-tab-kind][data-tab-id]') as HTMLElement | null;
  if (!tab || tab.dataset.tabKind !== kind) return null;
  return tab.dataset.tabId ?? null;
}

async function pollOutside(): Promise<void> {
  if (!active?.engaged || active.outside) return;
  try {
    const [cursor, position, size] = await Promise.all([
      cursorPosition(),
      getCurrentWindow().outerPosition(),
      getCurrentWindow().outerSize(),
    ]);
    if (!active || active.outside) return;
    const localX = cursor.x - position.x;
    const localY = cursor.y - position.y;
    const width = size.width;
    const height = size.height;
    if (localX >= 0 && localX <= width && localY >= 0 && localY <= height) return;

    // Pointer left the main window: hand this tab to a real OS window that
    // follows the cursor through the native drag loop.
    active.outside = true;
    const { onTearOut } = active.start;
    finishDrag();
    try {
      const label = await onTearOut({ x: cursor.x, y: cursor.y });
      if (typeof label === 'string' && label) {
        const detached = await WebviewWindow.getByLabel(label);
        await detached?.startDragging();
      }
    } catch (error) {
      console.error('[TabDrag] Tear-out failed:', error);
    }
  } catch (error) {
    console.error('[TabDrag] Outside-detection failed:', error);
  }
}

function handleMouseMove(event: MouseEvent) {
  if (!active) return;
  if (!active.engaged) {
    const dx = event.clientX - active.startX;
    const dy = event.clientY - active.startY;
    if (dx * dx + dy * dy < ENGAGE_THRESHOLD_PX * ENGAGE_THRESHOLD_PX) return;
    active.engaged = true;
    setBodyDragging(true);
    if (pollTimer === null) {
      pollTimer = window.setInterval(() => void pollOutside(), 24);
    }
  }
  if (active.outside) return;

  event.preventDefault();
  clearPaneHighlight();

  const reorderId = active.start.onReorderOver
    ? reorderTargetUnder(event.clientX, event.clientY, active.start.kind)
    : null;
  if (reorderId && reorderId !== active.start.id) {
    active.start.onReorderOver?.(reorderId);
    return;
  }

  if (active.start.onPaneDrop) {
    const edge = paneEdgeUnder(event.clientX, event.clientY);
    if (edge) {
      const pane = document.querySelector(`[data-pane-id="${CSS.escape(edge.paneId)}"]`);
      pane?.classList.add('pane-drop-hover');
    }
  }
}

function finishDrag() {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
  clearPaneHighlight();
  setBodyDragging(false);
  active = null;
}

function handleMouseUp(event: MouseEvent) {
  const state = active;
  if (!state) return;
  const wasEngaged = state.engaged;
  finishDrag();
  if (!wasEngaged || state.outside) return;

  if (state.start.onPaneDrop) {
    const edge = paneEdgeUnder(event.clientX, event.clientY);
    if (edge) {
      state.start.onPaneDrop(edge.paneId, edge.direction);
      return;
    }
  }
}

function handleKeyDown(event: KeyboardEvent) {
  if (event.key === 'Escape' && active) {
    finishDrag();
  }
}

let listenersInstalled = false;

function installListeners() {
  if (listenersInstalled || typeof document === 'undefined') return;
  listenersInstalled = true;
  document.addEventListener('mousemove', handleMouseMove);
  document.addEventListener('mouseup', handleMouseUp);
  document.addEventListener('blur', () => finishDrag());
  document.addEventListener('keydown', handleKeyDown);
}
installListeners();

/**
 * Attach to a tab chip's onMouseDown. Returns early for non-primary buttons
 * and clicks on inner action buttons (close / detach) so they behave normally.
 */
export function beginTabDragOnMouseDown(event: React.MouseEvent, start: TabDragStart): void {
  if (event.button !== 0) return;
  if ((event.target as HTMLElement).closest('button, input, select, a')) return;
  active = {
    start,
    startX: event.clientX,
    startY: event.clientY,
    engaged: false,
    outside: false,
  };
}
