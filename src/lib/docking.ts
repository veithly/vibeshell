import type { MosaicNode } from 'react-mosaic-component2';
import { countLeaves, getLeaves, MAX_TERMINAL_PANES, removePane } from './mosaicTree';

export type DockSide = 'left' | 'right' | 'top' | 'bottom';
export interface PanePlacement { paneId: string; side: DockSide }
export interface Rect { x: number; y: number; width: number; height: number }

export function sideAt(x: number, y: number, rect: Rect): DockSide {
  const rx = (x - rect.x) / Math.max(rect.width, 1);
  const ry = (y - rect.y) / Math.max(rect.height, 1);
  const distances: [DockSide, number][] = [['left', rx], ['right', 1 - rx], ['top', ry], ['bottom', 1 - ry]];
  // Middle drops put a view beside the current one, not on top of it.
  if (Math.min(...distances.map(([, distance]) => distance)) > 0.3) return 'right';
  return distances.reduce((a, b) => b[1] < a[1] ? b : a)[0];
}

export function replaceLeaf(
  tree: MosaicNode<string> | null, id: string, replacement: MosaicNode<string>
): MosaicNode<string> | null {
  if (tree === null) return null;
  if (typeof tree === 'string') return tree === id ? replacement : tree;
  const first = replaceLeaf(tree.first, id, replacement)!;
  const second = replaceLeaf(tree.second, id, replacement)!;
  return first === tree.first && second === tree.second ? tree : { ...tree, first, second };
}

/** Moving a pane is remove+insert, never clone; a missing target is a no-op. */
export function dockPane(
  tree: MosaicNode<string> | null, target: string, source: string, side: DockSide
): MosaicNode<string> | null {
  if (tree === null) return source;
  if (target === source || !getLeaves(tree).includes(target)) return tree;
  const moving = getLeaves(tree).includes(source);
  if (!moving && countLeaves(tree) >= MAX_TERMINAL_PANES) return tree;
  const pruned = moving ? removePane(tree, source) : tree;
  const before = side === 'left' || side === 'top';
  return replaceLeaf(pruned, target, {
    direction: side === 'left' || side === 'right' ? 'row' : 'column',
    first: before ? source : target,
    second: before ? target : source,
    splitPercentage: 50,
  });
}

/** Reject malformed/cyclic/deep layouts and collapse stale/duplicate leaves. */
export function sanitizeTree(value: unknown, validIds?: Set<string>): MosaicNode<string> | null {
  const seen = new Set<string>();
  function visit(node: unknown, depth: number): MosaicNode<string> | null {
    if (depth > 20 || seen.size >= MAX_TERMINAL_PANES) return null;
    if (typeof node === 'string') {
      if (!node || node.length > 8192 || seen.has(node) || (validIds && !validIds.has(node))) return null;
      seen.add(node);
      return node;
    }
    if (!node || typeof node !== 'object') return null;
    const branch = node as Record<string, unknown>;
    if (branch.direction !== 'row' && branch.direction !== 'column') return null;
    const first = visit(branch.first, depth + 1);
    const second = visit(branch.second, depth + 1);
    if (!first || !second) return first ?? second;
    return {
      direction: branch.direction, first, second,
      splitPercentage: typeof branch.splitPercentage === 'number' && Number.isFinite(branch.splitPercentage)
        ? Math.max(1, Math.min(99, branch.splitPercentage)) : 50,
    };
  }
  return visit(value, 0);
}

export function paneAtPoint(x: number, y: number): PanePlacement | null {
  const pane = document.elementFromPoint?.(x, y)?.closest<HTMLElement>('[data-pane-id]');
  if (!pane?.dataset.paneId) return null;
  const rect = pane.getBoundingClientRect();
  return { paneId: pane.dataset.paneId, side: sideAt(x, y, { x: rect.left, y: rect.top, width: rect.width, height: rect.height }) };
}

export function clearDockPreview(): void {
  document.querySelectorAll<HTMLElement>('[data-dock-side]').forEach((element) => {
    delete element.dataset.dockSide;
    element.classList.remove('pane-drop-hover');
  });
  document.querySelectorAll('.dock-tab-hover').forEach((element) => element.classList.remove('dock-tab-hover'));
}

export function showDockPreview(placement: PanePlacement | null): void {
  clearDockPreview();
  if (!placement) return;
  // Avoid interpolating externally supplied ids into a CSS selector.
  const pane = [...document.querySelectorAll<HTMLElement>('[data-pane-id]')]
    .find((element) => element.dataset.paneId === placement.paneId);
  if (pane) {
    pane.dataset.dockSide = placement.side;
    pane.classList.add('pane-drop-hover');
  }
}

/** All stored geometry is physical pixels; clamp to a currently connected display. */
export function fitWindowRect(rect: Rect, monitors: Rect[], minimum = { width: 480, height: 320 }): Rect {
  if (!monitors.length) return rect;
  const area = (m: Rect) => Math.max(0, Math.min(rect.x + rect.width, m.x + m.width) - Math.max(rect.x, m.x))
    * Math.max(0, Math.min(rect.y + rect.height, m.y + m.height) - Math.max(rect.y, m.y));
  const monitor = monitors.reduce((a, b) => area(b) > area(a) ? b : a);
  const width = Math.min(monitor.width, Math.max(minimum.width, rect.width));
  const height = Math.min(monitor.height, Math.max(minimum.height, rect.height));
  return { width, height,
    x: Math.max(monitor.x, Math.min(rect.x, monitor.x + monitor.width - width)),
    y: Math.max(monitor.y, Math.min(rect.y, monitor.y + monitor.height - height)),
  };
}
