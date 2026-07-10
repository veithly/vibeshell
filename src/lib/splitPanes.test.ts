import { describe, expect, it } from 'vitest';
import { addTerminalPane, removeTerminalPane, syncTerminalPanes } from './splitPanes';

describe('terminal split panes', () => {
  it('starts with the active session and replaces the primary pane when switching tabs', () => {
    expect(syncTerminalPanes([], ['a', 'b'], 'a')).toEqual(['a']);
    expect(syncTerminalPanes(['a', 'b'], ['a', 'b', 'c'], 'c')).toEqual(['c', 'b']);
  });

  it('keeps visible secondary panes and removes closed sessions', () => {
    expect(syncTerminalPanes(['a', 'b', 'c'], ['a', 'c'], 'a')).toEqual(['a', 'c']);
  });

  it('adds at most four unique panes and removes panes without killing sessions', () => {
    expect(addTerminalPane(['a', 'b', 'c', 'd'], 'e')).toEqual(['a', 'b', 'c', 'd']);
    expect(addTerminalPane(['a'], 'b')).toEqual(['a', 'b']);
    expect(removeTerminalPane(['a', 'b'], 'a')).toEqual(['b']);
  });
});
