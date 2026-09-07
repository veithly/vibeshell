import { emitTo, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { physicalPosition } from './physicalPixels';
import { safeInvoke } from './tauri';
import { clearDockPreview, paneAtPoint, showDockPreview, type PanePlacement } from './docking';
import { detachTargetKey, useDetachedOwnership, type TransferSnapshot } from './detach';

const PREVIEW_EVENT = 'vibeshell://dock-preview';
const DROP_EVENT = 'vibeshell://dock-drop';
const ACK_EVENT = 'vibeshell://dock-ack';
export interface PointerState { x: number; y: number; primaryDown: boolean | null; escapeDown: boolean }
interface DropRequest { id: string; source: string; snapshot: TransferSnapshot; expiresAt: number; point?: { x: number; y: number } }

export async function requestDock(snapshot: TransferSnapshot, point?: { x: number; y: number }): Promise<boolean> {
  const request: DropRequest = { id: crypto.randomUUID(), source: getCurrentWindow().label, snapshot, point, expiresAt: Date.now() + 7000 };
  let stop: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  try {
    return await new Promise<boolean>((resolve, reject) => {
      let attempts = 0;
      void listen<{ id: string; accepted: boolean }>(ACK_EVENT, ({ payload }) => {
        if (payload.id === request.id) resolve(payload.accepted);
      }).then((unlisten) => {
        stop = unlisten;
        const send = () => {
          if (++attempts > 4) { reject(new Error('Main window did not confirm the transfer')); return; }
          void emitTo('main', DROP_EVENT, request).catch(reject);
        };
        timer = setInterval(send, 2000);
        send();
      }).catch(reject);
    });
  } finally { stop?.(); clearInterval(timer); }
}

export async function receiveWindowDrops(
  accept: (snapshot: TransferSnapshot, placement: PanePlacement | null) => boolean
): Promise<() => void> {
  if (!('__TAURI_INTERNALS__' in window)) return () => {};
  const native = getCurrentWindow();
  const decisions = new Map<string, boolean>();
  const pendingDecisions = new Set<string>();
  let sequence = 0;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  const hit = async (point: { x: number; y: number }) => {
    const [origin, scale] = await Promise.all([native.innerPosition(), native.scaleFactor()]);
    const x = (point.x - origin.x) / scale;
    const y = (point.y - origin.y) / scale;
    if (x < 0 || y < 0 || x >= window.innerWidth || y >= window.innerHeight) return { valid: false, pane: null };
    const element = document.elementFromPoint?.(x, y);
    const pane = paneAtPoint(x, y);
    return { valid: !!pane || !!element?.closest('.session-tabbar, .workspace-return-zone'), pane };
  };
  const stopPreview = await listen<{ source: string; x?: number; y?: number }>(PREVIEW_EVENT, async ({ payload }) => {
    const current = ++sequence;
    if (!Object.values(useDetachedOwnership.getState().owners).includes(payload.source)) return;
    clearTimeout(previewTimer);
    if (payload.x === undefined || payload.y === undefined) { clearDockPreview(); return; }
    try {
      const target = await hit({ x: payload.x, y: payload.y });
      if (current !== sequence) return;
      showDockPreview(target.pane);
      if (target.valid && !target.pane) document.querySelector('.session-tabbar')?.classList.add('dock-tab-hover');
      previewTimer = setTimeout(clearDockPreview, 500);
    } catch { clearDockPreview(); }
  });
  const stopDrop = await listen<DropRequest>(DROP_EVENT, async ({ payload }) => {
    if (!payload?.id || !payload.snapshot?.target) return;
    let accepted = decisions.get(payload.id);
    if (accepted === undefined) {
      if (pendingDecisions.has(payload.id)) return;
      pendingDecisions.add(payload.id);
      const key = detachTargetKey(payload.snapshot.target);
      accepted = false;
      if (useDetachedOwnership.getState().owners[key] === payload.source) {
        try {
          const target = payload.point ? await hit(payload.point) : { valid: true, pane: null };
          if (target.valid && payload.expiresAt > Date.now()
            && useDetachedOwnership.getState().owners[key] === payload.source) accepted = accept(payload.snapshot, target.pane);
        } catch (error) { console.error('[Workspace] Rejected window transfer:', error); }
      }
      pendingDecisions.delete(payload.id);
      decisions.set(payload.id, accepted);
      if (decisions.size > 100) decisions.delete(decisions.keys().next().value!);
    }
    sequence++;
    clearDockPreview();
    await emitTo(payload.source, ACK_EVENT, { id: payload.id, accepted }).catch(console.error);
    if (accepted) await native.setFocus().catch(console.error);
  });
  return () => { sequence++; clearTimeout(previewTimer); stopPreview(); stopDrop(); clearDockPreview(); };
}

let dragging = false;
/** Runs only for the duration of this explicit gesture; no movement-debounce guesses. */
export async function dragNativeWindow(
  onDrop: (point: { x: number; y: number }) => Promise<void>,
  tearOut = false,
  signal?: AbortSignal
): Promise<void> {
  if (dragging) return;
  dragging = true;
  const native = getCurrentWindow();
  let releasedInWebview = false;
  let cancelledInWebview = false;
  const release = () => { releasedInWebview = true; };
  const cancel = (event: KeyboardEvent) => { if (event.key === 'Escape') cancelledInWebview = true; };
  document.addEventListener('mouseup', release);
  document.addEventListener('keydown', cancel);
  try {
    const first = await safeInvoke<PointerState>('workspace_pointer_state');
    if (!first.success) throw first.error;
    const original = await native.outerPosition();
    const scale = await native.scaleFactor();
    const offset = tearOut ? { x: 90 * scale, y: 18 * scale }
      : { x: first.data.x - original.x, y: first.data.y - original.y };
    if (tearOut) await native.setPosition(physicalPosition(first.data.x - offset.x, first.data.y - offset.y));
    if (first.data.primaryDown === false) return;
    // Platforms without a native button query still support normal OS dragging.
    // Do not guess when it ended: returning to main remains available via its button.
    if (first.data.primaryDown === null) { await native.startDragging(); return; }
    document.body.classList.add('detached-window-dragging');
    let point = first.data;
    while (!signal?.aborted) {
      if (point.escapeDown || cancelledInWebview) {
        await native.setPosition(physicalPosition(original.x, original.y));
        return;
      }
      if (!point.primaryDown || releasedInWebview) { await onDrop(point); return; }
      await native.setPosition(physicalPosition(point.x - offset.x, point.y - offset.y));
      await emitTo('main', PREVIEW_EVENT, { source: native.label, x: point.x, y: point.y });
      await new Promise((resolve) => setTimeout(resolve, 16));
      const current = await safeInvoke<PointerState>('workspace_pointer_state');
      if (!current.success) throw current.error;
      point = current.data;
    }
  } finally {
    dragging = false;
    document.body.classList.remove('detached-window-dragging');
    document.removeEventListener('mouseup', release);
    document.removeEventListener('keydown', cancel);
    await emitTo('main', PREVIEW_EVENT, { source: native.label }).catch(console.error);
  }
}
