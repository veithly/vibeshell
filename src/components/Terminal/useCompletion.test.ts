import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useCompletion } from './useCompletion';

describe('useCompletion automatic suggestions', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('shows matching suggestions once the input reaches the trigger length', () => {
    const { result } = renderHook(() => useCompletion());

    // autoTrigger is debounced so rapid keystrokes don't block the main thread.
    // The popup is not visible synchronously after the call.
    act(() => {
      result.current[1].autoTrigger('gi', { x: 24, y: 48 });
    });
    expect(result.current[0].visible).toBe(false);

    // After the debounce window elapses, suggestions appear.
    act(() => {
      vi.advanceTimersByTime(200);
    });

    expect(result.current[0].visible).toBe(true);
    expect(result.current[0].items.some((item) => item.text === 'git')).toBe(true);
    expect(result.current[0].position).toEqual({ x: 24, y: 48 });
  });

  it('keeps the popup hidden when no local suggestion matches', () => {
    const { result } = renderHook(() => useCompletion());

    act(() => {
      result.current[1].autoTrigger('zzzz-no-command', { x: 8, y: 16 });
    });

    act(() => {
      vi.advanceTimersByTime(200);
    });

    expect(result.current[0].visible).toBe(false);
    expect(result.current[0].items).toHaveLength(0);
  });

  it('clears below-threshold input synchronously without waiting for debounce', () => {
    const { result } = renderHook(() => useCompletion());

    // First trigger a visible popup.
    act(() => {
      result.current[1].autoTrigger('gi', { x: 24, y: 48 });
    });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(result.current[0].visible).toBe(true);

    // Deleting back below the trigger threshold should clear synchronously.
    act(() => {
      result.current[1].autoTrigger('g', { x: 24, y: 48 });
    });
    expect(result.current[0].visible).toBe(false);
    expect(result.current[0].items).toHaveLength(0);
  });
});
