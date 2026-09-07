import { describe, expect, it } from 'vitest';
import { physicalPosition, physicalSize } from './physicalPixels';

describe('native physical pixels', () => {
  it('rounds the reported fractional tear-out coordinate', () => {
    expect(physicalPosition(1277.640625, 67.125)).toMatchObject({ x: 1278, y: 67 });
  });
  it('keeps negative monitor coordinates and supports fractional scales', () => {
    expect(physicalPosition(-1277.640625, 310.125 - 18 * 1.25)).toMatchObject({ x: -1278, y: 288 });
  });
  it('bounds coordinates to i32 and dimensions to nonzero u32', () => {
    expect(physicalPosition(-1e20, 1e20)).toMatchObject({ x: -2147483648, y: 2147483647 });
    expect(physicalSize(0, 700.7)).toMatchObject({ width: 1, height: 701 });
  });
  it('rejects nonfinite input before IPC', () => {
    expect(() => physicalPosition(NaN, 0)).toThrow(RangeError);
    expect(() => physicalSize(Infinity, 10)).toThrow(RangeError);
  });
});
