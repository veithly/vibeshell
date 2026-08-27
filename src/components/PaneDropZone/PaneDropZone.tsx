import type { ReactNode } from 'react';

/**
 * Marks a mosaic pane as a drop target for the mouse-driven tab drag
 * controller (src/lib/tabDragController.ts), which hit-tests
 * `[data-pane-id]` elements under the cursor and toggles
 * `.pane-drop-hover` while hovering an edge.
 */
export function PaneDropZone({ paneId, children }: { paneId: string; children: ReactNode }) {
  return (
    <div className="relative h-full min-h-0 min-w-0" data-pane-id={paneId}>
      {children}
    </div>
  );
}
