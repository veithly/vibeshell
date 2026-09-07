export interface TextEditBuffer {
  text: string;
  saved: string;
  truncated: boolean;
  size: number;
  mimeType: string;
}
const PREFIX = 'vibeshell.editor-buffer.v1.';
const memory = new Map<string, TextEditBuffer>();
const listeners = new Map<string, Set<() => void>>();
const locks = new Set<string>();
const saves = new Map<string, { done: Promise<void>; finish: () => void }>();
const publish = (id: string) => listeners.get(id)?.forEach((listener) => listener());

export function subscribeTextBuffer(id: string, listener: () => void): () => void {
  const group = listeners.get(id) ?? new Set<() => void>();
  group.add(listener); listeners.set(id, group);
  return () => { group.delete(listener); if (!group.size) listeners.delete(id); };
}
export function isTextBufferLocked(id: string): boolean { return locks.has(id); }
export function isTextBufferSaving(id: string): boolean { return saves.has(id); }
export function setTextBufferLocked(id: string, locked: boolean): void {
  if (locked) locks.add(id); else locks.delete(id);
  publish(id);
}
export function beginTextSave(id: string): boolean {
  if (saves.has(id) || locks.has(id)) return false;
  let finish!: () => void;
  const done = new Promise<void>((resolve) => { finish = resolve; });
  saves.set(id, { done, finish }); publish(id);
  return true;
}
export function endTextSave(id: string): void {
  const save = saves.get(id); saves.delete(id); save?.finish(); publish(id);
}
export async function waitForTextSave(id: string): Promise<void> {
  const save = saves.get(id);
  if (!save) return;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([save.done, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('File save is still in progress; the tab has not been moved.')), 15000);
    })]);
  } finally { clearTimeout(timer); }
}

export function readTextBuffer(id: string): TextEditBuffer | null {
  if (memory.has(id)) return memory.get(id)!;
  try {
    const raw = window.localStorage.getItem(PREFIX + encodeURIComponent(id));
    const value: unknown = raw ? JSON.parse(raw) : null;
    if (!value || typeof value !== 'object') return null;
    const buffer = value as TextEditBuffer;
    if (typeof buffer.text !== 'string' || typeof buffer.saved !== 'string'
      || typeof buffer.truncated !== 'boolean' || !Number.isFinite(buffer.size)) return null;
    memory.set(id, buffer);
    return buffer;
  } catch { return null; }
}

/** Layout changes never write to the user's local/remote file. This is hot-exit recovery only. */
export function writeTextBuffer(id: string, buffer: TextEditBuffer): boolean {
  memory.set(id, buffer);
  publish(id);
  try {
    // Clean buffers need no disk copy: they can safely be read from the file again.
    const key = PREFIX + encodeURIComponent(id);
    if (buffer.text === buffer.saved) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, JSON.stringify(buffer));
    return true;
  } catch { return false; }
}

export function forgetTextBuffer(id: string): void {
  memory.delete(id);
  try { window.localStorage.removeItem(PREFIX + encodeURIComponent(id)); } catch { /* storage disabled */ }
}

export function refreshTextBuffer(id: string): TextEditBuffer | null {
  memory.delete(id);
  return readTextBuffer(id);
}
