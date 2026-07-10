import { describe, expect, it } from 'vitest';
import { applyTrackedInput, getClickedInputPosition, getCursorMoveSequence } from './inputCursor';

describe('terminal input cursor tracking', () => {
  it('inserts and deletes at the tracked cursor', () => {
    expect(applyTrackedInput('helo', 2, 'l')).toEqual({
      buffer: 'hello',
      cursor: 3,
      known: true,
    });
    expect(applyTrackedInput('hello', 3, '\x7f')).toEqual({
      buffer: 'helo',
      cursor: 2,
      known: true,
    });
  });

  it('tracks shell navigation keys without discarding the command', () => {
    expect(applyTrackedInput('deploy prod', 11, '\x1b[D')).toEqual({
      buffer: 'deploy prod',
      cursor: 10,
      known: true,
    });
    expect(applyTrackedInput('deploy prod', 4, '\x05').cursor).toBe(11);
    expect(applyTrackedInput('deploy prod', 4, '\x01').cursor).toBe(0);
  });

  it('maps terminal cells to a clamped command position', () => {
    expect(getClickedInputPosition({
      clickColumn: 10,
      clickRow: 4,
      cursorColumn: 15,
      cursorRow: 4,
      terminalColumns: 80,
      inputLength: 12,
      inputCursor: 12,
    })).toBe(7);
    expect(getClickedInputPosition({
      clickColumn: 2,
      clickRow: 3,
      cursorColumn: 78,
      cursorRow: 2,
      terminalColumns: 80,
      inputLength: 20,
      inputCursor: 18,
    })).toBe(20);
  });

  it('builds shell-compatible cursor key sequences', () => {
    expect(getCursorMoveSequence(5, 2)).toBe('\x1b[D\x1b[D\x1b[D');
    expect(getCursorMoveSequence(2, 4)).toBe('\x1b[C\x1b[C');
  });
});
