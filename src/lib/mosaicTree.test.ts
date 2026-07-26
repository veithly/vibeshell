import { describe, it, expect } from 'vitest';
import type { MosaicNode } from 'react-mosaic-component2';
import {
  addPane,
  removePane,
  getLeaves,
  pruneLeaves,
  countLeaves,
  MAX_TERMINAL_PANES,
} from './mosaicTree';

describe('addPane', () => {
  it('creates a single leaf from null', () => {
    expect(addPane(null, 'a', 'b', 'row')).toBe('b');
  });

  it('splits a matching leaf into a row parent', () => {
    const tree: MosaicNode<string> = 'a';
    const result = addPane(tree, 'a', 'b', 'row');
    expect(result).toEqual({
      direction: 'row',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    });
  });

  it('splits a matching leaf into a column parent', () => {
    const result = addPane('a', 'a', 'b', 'column');
    expect(result).toEqual({
      direction: 'column',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    });
  });

  it('recurses into the correct child and keeps the other untouched', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    };
    const result = addPane(tree, 'b', 'c', 'column');
    expect(result).toEqual({
      direction: 'row',
      first: 'a',
      second: {
        direction: 'column',
        first: 'b',
        second: 'c',
        splitPercentage: 50,
      },
      splitPercentage: 50,
    });
  });

  it('returns the original tree when leafId is not found', () => {
    const tree: MosaicNode<string> = 'a';
    expect(addPane(tree, 'x', 'b', 'row')).toBe(tree);
  });
});

describe('removePane', () => {
  it('returns null when removing the only leaf', () => {
    expect(removePane('a', 'a')).toBeNull();
  });

  it('collapses a parent to its surviving child', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    };
    expect(removePane(tree, 'b')).toBe('a');
    expect(removePane(tree, 'a')).toBe('b');
  });

  it('returns the tree unchanged when id is absent', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    };
    expect(removePane(tree, 'z')).toBe(tree);
  });

  it('collapses nested parents correctly', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: {
        direction: 'column',
        first: 'b',
        second: 'c',
        splitPercentage: 50,
      },
      splitPercentage: 50,
    };
    // Remove 'b': inner column collapses to 'c', outer row stays.
    expect(removePane(tree, 'b')).toEqual({
      direction: 'row',
      first: 'a',
      second: 'c',
      splitPercentage: 50,
    });
  });
});

describe('getLeaves / countLeaves', () => {
  it('returns empty for null', () => {
    expect(getLeaves(null)).toEqual([]);
    expect(countLeaves(null)).toBe(0);
  });

  it('returns a single leaf', () => {
    expect(getLeaves('a')).toEqual(['a']);
    expect(countLeaves('a')).toBe(1);
  });

  it('collects leaves depth-first left-to-right', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: {
        direction: 'column',
        first: 'b',
        second: 'c',
        splitPercentage: 50,
      },
      splitPercentage: 50,
    };
    expect(getLeaves(tree)).toEqual(['a', 'b', 'c']);
    expect(countLeaves(tree)).toBe(3);
  });
});

describe('pruneLeaves', () => {
  it('returns null when the single leaf is invalid', () => {
    expect(pruneLeaves('a', new Set(['b']))).toBeNull();
  });

  it('keeps a valid single leaf', () => {
    expect(pruneLeaves('a', new Set(['a']))).toBe('a');
  });

  it('removes invalid leaves and collapses parents', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: {
        direction: 'column',
        first: 'b',
        second: 'c',
        splitPercentage: 50,
      },
      splitPercentage: 50,
    };
    // 'b' is invalid -> inner column collapses to 'c'.
    expect(pruneLeaves(tree, new Set(['a', 'c']))).toEqual({
      direction: 'row',
      first: 'a',
      second: 'c',
      splitPercentage: 50,
    });
  });

  it('returns null when all leaves are invalid', () => {
    const tree: MosaicNode<string> = {
      direction: 'row',
      first: 'a',
      second: 'b',
      splitPercentage: 50,
    };
    expect(pruneLeaves(tree, new Set(['z']))).toBeNull();
  });
});

describe('MAX_TERMINAL_PANES', () => {
  it('is 9', () => {
    expect(MAX_TERMINAL_PANES).toBe(9);
  });
});
