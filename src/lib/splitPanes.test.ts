import { describe, expect, it } from 'vitest';
import {
  addTerminalPane,
  getTerminalGridTracks,
  removeTerminalPane,
  syncTerminalPanes,
} from './splitPanes';

describe('terminal split panes', () => {
  it('starts with the active session and replaces the primary pane when switching tabs', () => {
    expect(syncTerminalPanes([], ['a', 'b'], 'a')).toEqual(['a']);
    expect(syncTerminalPanes(['a', 'b'], ['a', 'b', 'c'], 'c')).toEqual(['c', 'b']);
  });

  it('keeps visible secondary panes and removes closed sessions', () => {
    expect(syncTerminalPanes(['a', 'b', 'c'], ['a', 'c'], 'a')).toEqual(['a', 'c']);
  });

  it('adds at most nine unique panes and removes panes without killing sessions', () => {
    expect(addTerminalPane(['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i'], 'j')).toEqual([
      'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i',
    ]);
    expect(addTerminalPane(['a'], 'b')).toEqual(['a', 'b']);
    expect(removeTerminalPane(['a', 'b'], 'a')).toEqual(['b']);
  });

  it('uses compact grid presets for four, six, and nine panes', () => {
    expect(getTerminalGridTracks('grid', 4)).toEqual({ columns: 2, rows: 2 });
    expect(getTerminalGridTracks('grid', 6)).toEqual({ columns: 3, rows: 2 });
    expect(getTerminalGridTracks('grid', 9)).toEqual({ columns: 3, rows: 3 });
  });

  it('preserves explicit horizontal and vertical layouts', () => {
    expect(getTerminalGridTracks('columns', 3)).toEqual({ columns: 3, rows: 1 });
    expect(getTerminalGridTracks('rows', 3)).toEqual({ columns: 1, rows: 3 });
  });
});
