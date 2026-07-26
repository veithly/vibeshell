import type { MosaicNode, MosaicDirection } from 'react-mosaic-component2';

export const MAX_TERMINAL_PANES = 9;

/**
 * Split `leafId` into a new parent containing the existing leaf and `newId`,
 * arranged according to `direction`. The existing leaf stays in the `first`
 * position so its terminal keeps its place; the new pane appears on the right
 * (row) or bottom (column).
 *
 * Returns the new tree, or the original tree if `leafId` is not found.
 */
export function addPane<T extends string>(
  tree: MosaicNode<T> | null,
  leafId: T,
  newId: T,
  direction: MosaicDirection
): MosaicNode<T> | null {
  if (tree === null) {
    return newId;
  }

  // Leaf node: if it matches, split it.
  if (typeof tree === 'string') {
    if (tree === leafId) {
      return {
        direction,
        first: tree,
        second: newId,
        splitPercentage: 50,
      };
    }
    return tree;
  }

  // Parent node: recurse into both children. addPane only returns null for a
  // null input, and tree.first/tree.second are non-null, so the casts are safe.
  const newFirst = addPane(tree.first, leafId, newId, direction) ?? tree.first;
  const newSecond = addPane(tree.second, leafId, newId, direction) ?? tree.second;

  // Only rebuild if something changed in a child.
  if (newFirst === tree.first && newSecond === tree.second) {
    return tree;
  }
  return { ...tree, first: newFirst, second: newSecond };
}

/**
 * Remove the leaf `id` from the tree, collapsing any parent that is left with
 * a single child. Returns the new tree (or null if it becomes empty).
 */
export function removePane<T extends string>(
  tree: MosaicNode<T> | null,
  id: T
): MosaicNode<T> | null {
  if (tree === null) return null;

  if (typeof tree === 'string') {
    return tree === id ? null : tree;
  }

  const newFirst = removePane(tree.first, id);
  const newSecond = removePane(tree.second, id);

  // Both children survived: keep the parent.
  if (newFirst !== null && newSecond !== null) {
    if (newFirst === tree.first && newSecond === tree.second) {
      return tree;
    }
    return { ...tree, first: newFirst, second: newSecond };
  }

  // One child survived: collapse the parent to that child.
  return newFirst ?? newSecond;
}

/**
 * Collect all leaf ids in the tree (left-to-right, depth-first).
 */
export function getLeaves<T extends string>(tree: MosaicNode<T> | null): T[] {
  if (tree === null) return [];
  if (typeof tree === 'string') return [tree];
  return [...getLeaves(tree.first), ...getLeaves(tree.second)];
}

/**
 * Remove any leaf whose id is not in `validIds`. Used to prune sessions that
 * have been closed from the tree without tearing down the whole layout.
 */
export function pruneLeaves<T extends string>(
  tree: MosaicNode<T> | null,
  validIds: Set<T>
): MosaicNode<T> | null {
  if (tree === null) return null;

  if (typeof tree === 'string') {
    return validIds.has(tree) ? tree : null;
  }

  const newFirst = pruneLeaves(tree.first, validIds);
  const newSecond = pruneLeaves(tree.second, validIds);

  if (newFirst !== null && newSecond !== null) {
    if (newFirst === tree.first && newSecond === tree.second) {
      return tree;
    }
    return { ...tree, first: newFirst, second: newSecond };
  }

  return newFirst ?? newSecond;
}

/**
 * Count leaves in the tree.
 */
export function countLeaves<T extends string>(tree: MosaicNode<T> | null): number {
  return getLeaves(tree).length;
}
