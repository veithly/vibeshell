import { PhysicalPosition, PhysicalSize } from '@tauri-apps/api/dpi';

function integerPixel(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) throw new RangeError('Window coordinates must be finite');
  return Math.max(min, Math.min(max, Math.round(value)));
}

/** Tauri's Rust physical position uses i32 even though JS accepts any number. */
export function physicalPosition(x: number, y: number): PhysicalPosition {
  return new PhysicalPosition(integerPixel(x, -2147483648, 2147483647), integerPixel(y, -2147483648, 2147483647));
}

/** Physical dimensions are non-zero u32 values; round at the IPC boundary. */
export function physicalSize(width: number, height: number): PhysicalSize {
  return new PhysicalSize(integerPixel(width, 1, 4294967295), integerPixel(height, 1, 4294967295));
}
