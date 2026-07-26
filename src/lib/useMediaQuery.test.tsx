import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { useMediaQuery } from './useMediaQuery';

afterEach(cleanup);

describe('useMediaQuery', () => {
  it('updates when the compact breakpoint changes', () => {
    let matches = false;
    let listener: (() => void) | undefined;
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn().mockImplementation((query: string) => ({
        get matches() { return matches; },
        media: query,
        addEventListener: (_event: string, next: () => void) => { listener = next; },
        removeEventListener: vi.fn(),
      })),
    });

    const { result } = renderHook(() => useMediaQuery('(max-width: 767px)'));
    expect(result.current).toBe(false);

    act(() => {
      matches = true;
      listener?.();
    });
    expect(result.current).toBe(true);
  });
});
