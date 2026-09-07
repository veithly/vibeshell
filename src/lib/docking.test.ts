import { describe, expect, it } from 'vitest';
import type { MosaicNode } from 'react-mosaic-component2';
import { dockPane, fitWindowRect, replaceLeaf, sanitizeTree, sideAt, type DockSide } from './docking';
import { getLeaves, MAX_TERMINAL_PANES } from './mosaicTree';
import { filePaneId, parsePaneId } from './paneIds';

describe('workspace docking', () => {
  it.each<[DockSide, 'row' | 'column', string, string]>([
    ['left', 'row', 'new', 'old'], ['right', 'row', 'old', 'new'],
    ['top', 'column', 'new', 'old'], ['bottom', 'column', 'old', 'new'],
  ])('docks to the %s edge', (side, direction, first, second) => {
    expect(dockPane('old', 'old', 'new', side)).toEqual({ direction, first, second, splitPercentage: 50 });
  });
  it('moves an existing pane without duplicating it or changing unrelated split ratios', () => {
    const tree: MosaicNode<string> = { direction: 'row', first: 'a', splitPercentage: 37,
      second: { direction: 'column', first: 'b', second: 'c', splitPercentage: 68 } };
    const moved = dockPane(tree, 'b', 'a', 'bottom');
    expect(getLeaves(moved).sort()).toEqual(['a', 'b', 'c']);
    expect(moved).toEqual({ direction: 'column', splitPercentage: 68,
      first: { direction: 'column', first: 'b', second: 'a', splitPercentage: 50 }, second: 'c' });
    expect(replaceLeaf(tree, 'b', 'file')).toEqual({ ...tree,
      second: { direction: 'column', first: 'file', second: 'c', splitPercentage: 68 } });
  });
  it('rejects stale targets, self-drops and extra panes beyond the limit', () => {
    let tree: MosaicNode<string> | null = '0';
    for (let i = 1; i < MAX_TERMINAL_PANES; i++) tree = dockPane(tree, '0', String(i), 'right');
    expect(dockPane(tree, '0', 'extra', 'left')).toBe(tree);
    expect(dockPane(tree, 'missing', '0', 'right')).toBe(tree);
    expect(dockPane(tree, '0', '0', 'bottom')).toBe(tree);
    expect(getLeaves(dockPane(tree, '0', '1', 'left'))).toHaveLength(MAX_TERMINAL_PANES);
  });
  it.each<[number, number, DockSide]>([[105, 250, 'left'], [895, 250, 'right'], [500, 105, 'top'], [500, 495, 'bottom'], [500, 300, 'right']])('resolves actual pointer position %s,%s as %s', (x, y, side) => {
    expect(sideAt(x, y, { x: 100, y: 100, width: 800, height: 400 })).toBe(side);
  });
  it('restores valid narrow ratios, drops stale duplicates and tolerates malformed trees', () => {
    expect(sanitizeTree({ direction: 'row', first: 'a', second: 'b', splitPercentage: 7.125 }))
      .toEqual({ direction: 'row', first: 'a', second: 'b', splitPercentage: 7.125 });
    expect(sanitizeTree({ direction: 'row', first: 'a', second: 'a' })).toBe('a');
    expect(sanitizeTree({ direction: 'row', first: 'a', second: 'gone' }, new Set(['a']))).toBe('a');
    expect(sanitizeTree({ direction: 'diagonal', first: 'a', second: 'b' })).toBeNull();
    const cyclic: Record<string, unknown> = { direction: 'row', second: 'a' }; cyclic.first = cyclic;
    expect(sanitizeTree(cyclic)).toBe('a');
  });
  it('brings a window from a disconnected monitor back on screen', () => {
    const bounds = fitWindowRect({ x: 4000, y: -500, width: 980, height: 660 }, [{ x: 0, y: 24, width: 1440, height: 876 }]);
    expect(bounds).toEqual({ x: 460, y: 24, width: 980, height: 660 });
  });
  it('preserves positions on a connected monitor left of the primary display', () => {
    const rect = { x: -1700, y: 120, width: 900, height: 600 };
    expect(fitWindowRect(rect, [{ x: 0, y: 24, width: 1440, height: 876 }, { x: -1920, y: 0, width: 1920, height: 1080 }])).toEqual(rect);
  });
  it('round-trips unicode and control-separated file ids without CSS escaping assumptions', () => {
    const id = 'session\u0000/项目/a #b%[1].ts';
    const pane = filePaneId(id);
    expect(pane).not.toContain('\u0000');
    expect(parsePaneId(pane)).toEqual({ kind: 'file', id });
    expect(parsePaneId('file:%ZZ').kind).toBe('unknown');
  });
});
