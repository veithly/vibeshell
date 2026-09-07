import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { beginTabDragOnMouseDown, __internals } from './tabDragController';

function fireMouseEvent(type: string, options: MouseEventInit) {
  const { clientX = 0, clientY = 0, ...rest } = options;
  document.body.dispatchEvent(
    new MouseEvent(type, { bubbles: true, buttons: type === 'mousemove' ? 1 : 0, clientX, clientY, screenX: clientX, screenY: clientY, ...rest })
  );
}

function reactMouseDown(target: HTMLElement, clientX: number, clientY: number) {
  beginTabDragOnMouseDown(
    { button: 0, target, currentTarget: target, clientX, clientY } as unknown as React.MouseEvent,
    // assigned per test below
    (beginTabDragOnMouseDown as unknown as { __start?: unknown }).__start as never
  );
}

describe('tabDragController', () => {
  let chip: HTMLElement;

  beforeEach(() => {
    window.innerWidth = 1200;
    window.innerHeight = 800;
    chip = document.createElement('div');
    chip.dataset.tabKind = 'session';
    chip.dataset.tabId = 'session-1';
    document.body.appendChild(chip);
  });

  afterEach(() => {
    chip.remove();
    // Ensure no drag leaks between tests.
    fireMouseEvent('mouseup', { clientX: 400, clientY: 400 });
    document.body.className = '';
  });

  it('treats the title bar and window edges as tear-out zones', () => {
    const { isTearOutZone } = __internals;
    expect(isTearOutZone(600, 20)).toBe(true);            // title bar
    expect(isTearOutZone(5, 300)).toBe(true);             // left window edge
    expect(isTearOutZone(1197, 300)).toBe(true);          // right window edge
    expect(isTearOutZone(600, 796)).toBe(true);           // bottom window edge
    expect(isTearOutZone(10, 60)).toBe(true);             // left of the tab strip
    expect(isTearOutZone(600, 60)).toBe(false);           // inside the strip
    expect(isTearOutZone(600, 300)).toBe(false);          // workspace middle
  });

  it('tears out when the tab is dragged into the title bar', () => {
    const onTearOut = vi.fn();
    beginTabDragOnMouseDown(
      { button: 0, target: chip, currentTarget: chip, clientX: 600, clientY: 55 } as unknown as React.MouseEvent,
      { kind: 'session', id: 'session-1', onTearOut }
    );

    fireMouseEvent('mousemove', { clientX: 602, clientY: 40 });
    fireMouseEvent('mousemove', { clientX: 604, clientY: 25 });

    expect(onTearOut).toHaveBeenCalledTimes(1);
    expect(onTearOut.mock.calls[0][0]).toEqual(expect.objectContaining({ x: 604, y: 25 }));
  });

  it('ignores button-presses and plain clicks', () => {
    const onTearOut = vi.fn();
    beginTabDragOnMouseDown(
      { button: 2, target: chip, currentTarget: chip, clientX: 600, clientY: 55 } as unknown as React.MouseEvent,
      { kind: 'session', id: 'session-1', onTearOut }
    );
    fireMouseEvent('mousemove', { clientX: 604, clientY: 25 });
    expect(onTearOut).not.toHaveBeenCalled();

    beginTabDragOnMouseDown(
      { button: 0, target: chip, currentTarget: chip, clientX: 600, clientY: 55 } as unknown as React.MouseEvent,
      { kind: 'session', id: 'session-1', onTearOut }
    );
    fireMouseEvent('mousemove', { clientX: 601, clientY: 56 }); // below threshold
    expect(onTearOut).not.toHaveBeenCalled();
    expect(chip.classList.contains('tab-drag-source')).toBe(false);
  });

  it('dims the source chip while dragging and clears it on release', () => {
    const onTearOut = vi.fn();
    beginTabDragOnMouseDown(
      { button: 0, target: chip, currentTarget: chip, clientX: 600, clientY: 55 } as unknown as React.MouseEvent,
      { kind: 'session', id: 'session-1', onTearOut }
    );
    fireMouseEvent('mousemove', { clientX: 610, clientY: 60 });
    expect(chip.classList.contains('tab-drag-source')).toBe(true);
    expect(document.body.classList.contains('tab-dragging')).toBe(true);

    fireMouseEvent('mouseup', { clientX: 610, clientY: 60 });
    expect(chip.classList.contains('tab-drag-source')).toBe(false);
    expect(document.body.classList.contains('tab-dragging')).toBe(false);
    expect(onTearOut).not.toHaveBeenCalled();
  });

  it('splits a pane when released over one', () => {
    const pane = document.createElement('div');
    pane.dataset.paneId = 'session:session-1';
    document.body.appendChild(pane);
    const elementFromPoint = document.elementFromPoint;
    document.elementFromPoint = (() => pane) as typeof document.elementFromPoint;
    pane.getBoundingClientRect = () => ({ left: 0, top: 100, width: 1200, height: 700 } as DOMRect);

    const onPaneDrop = vi.fn();
    beginTabDragOnMouseDown(
      { button: 0, target: chip, currentTarget: chip, clientX: 600, clientY: 55 } as unknown as React.MouseEvent,
      { kind: 'plugin', id: 'session-1::docker-containers', onPaneDrop, onTearOut: vi.fn() }
    );
    fireMouseEvent('mousemove', { clientX: 600, clientY: 450 });
    fireMouseEvent('mouseup', { clientX: 600, clientY: 450 });

    // Dropping in the pane middle defaults to a row (beside) split.
    expect(onPaneDrop).toHaveBeenCalledWith('session:session-1', 'row', 'right');

    document.elementFromPoint = elementFromPoint;
    pane.remove();
  });
});

// Silence the unused helper warning; kept for future DOM-level tests.
void reactMouseDown;
