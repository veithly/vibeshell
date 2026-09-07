import type { MouseEvent as ReactMouseEvent } from 'react';
import { clearDockPreview, paneAtPoint, showDockPreview, type DockSide } from './docking';

export type TabKind = 'session' | 'file' | 'plugin';
export interface TabDragStart {
  kind: TabKind;
  id: string;
  onReorderOver?: (targetId: string) => void;
  onPaneDrop?: (paneId: string, direction: 'row' | 'column', side?: DockSide) => void;
  onTearOut: (at: { x: number; y: number }) => unknown;
}
interface Drag { start: TabDragStart; x: number; y: number; source: HTMLElement; engaged: boolean; lastReorder?: string }
let active: Drag | null = null;
let trailingClick: { source: HTMLElement; expires: number } | null = null;
function isTearOutZone(x: number, y: number): boolean {
  return y <= 34 || x <= (y < 92 ? 12 : 7) || x >= window.innerWidth - (y < 92 ? 12 : 7) || y >= window.innerHeight - 7;
}
function finishDrag(): Drag | null {
  const previous = active;
  active = null;
  if (previous?.engaged) trailingClick = { source: previous.source, expires: Date.now() + 150 };
  previous?.source.classList.remove('tab-drag-source');
  document.body.classList.remove('tab-dragging');
  clearDockPreview();
  return previous;
}
function onMouseMove(event: MouseEvent): void {
  const drag = active;
  if (!drag) return;
  if (event.buttons === 0) { finishDrag(); return; }
  if (!drag.engaged && Math.hypot(event.clientX - drag.x, event.clientY - drag.y) < 5) return;
  drag.engaged = true;
  drag.source.classList.add('tab-drag-source');
  document.body.classList.add('tab-dragging');
  if (isTearOutZone(event.clientX, event.clientY)) {
    finishDrag();
    void drag.start.onTearOut({ x: event.screenX, y: event.screenY });
    return;
  }
  const target = document.elementFromPoint?.(event.clientX, event.clientY)?.closest<HTMLElement>('[data-tab-kind]');
  if (target?.dataset.tabKind === drag.start.kind && target.dataset.tabId && target.dataset.tabId !== drag.start.id) {
    clearDockPreview();
    if (drag.lastReorder !== target.dataset.tabId) {
      drag.lastReorder = target.dataset.tabId;
      drag.start.onReorderOver?.(target.dataset.tabId);
    }
    return;
  }
  drag.lastReorder = undefined;
  showDockPreview(paneAtPoint(event.clientX, event.clientY));
}
function onMouseUp(event: MouseEvent): void {
  const placement = paneAtPoint(event.clientX, event.clientY);
  const drag = finishDrag();
  if (drag?.engaged && placement) {
    drag.start.onPaneDrop?.(placement.paneId, placement.side === 'left' || placement.side === 'right' ? 'row' : 'column', placement.side);
  }
}
const listeners = typeof document !== 'undefined' ? new AbortController() : null;
if (listeners) {
  const options = { signal: listeners.signal };
  document.addEventListener('mousemove', onMouseMove, options);
  document.addEventListener('mouseup', onMouseUp, options);
  document.addEventListener('mouseleave', (event) => {
    const drag = active;
    if (!drag || !(event.buttons & 1)) return;
    drag.engaged = true;
    finishDrag();
    void drag.start.onTearOut({ x: event.screenX, y: event.screenY });
  }, options);
  document.addEventListener('keydown', (event) => { if (event.key === 'Escape') finishDrag(); }, options);
  window.addEventListener('blur', () => finishDrag(), options);
  document.addEventListener('visibilitychange', () => { if (document.hidden) finishDrag(); }, options);
  document.addEventListener('click', (event) => {
    const pending = trailingClick;
    trailingClick = null;
    const target = event.target instanceof Element ? event.target : null;
    // Only suppress the click produced by releasing this drag, never a real
    // button, keyboard activation, or the next click elsewhere in the app.
    if (pending && event.detail !== 0 && Date.now() < pending.expires
      && target && pending.source.contains(target) && !target.closest('button, input, select, textarea, a')) {
      event.preventDefault(); event.stopPropagation();
    }
  }, { ...options, capture: true });
}
if (import.meta.hot) import.meta.hot.dispose(() => {
  listeners?.abort(); finishDrag(); trailingClick = null;
});
export function beginTabDragOnMouseDown(event: ReactMouseEvent, start: TabDragStart): void {
  if (event.button !== 0 || (event.target as Element).closest('button, input, select, textarea, a')) return;
  finishDrag();
  trailingClick = null;
  active = { start, x: event.clientX, y: event.clientY, source: event.currentTarget as HTMLElement, engaged: false };
}
export const __internals = { isTearOutZone, paneUnder: paneAtPoint };
