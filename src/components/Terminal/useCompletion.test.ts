import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useCompletion } from './useCompletion';

describe('useCompletion automatic suggestions', () => {
  it('shows matching suggestions once the input reaches the trigger length', () => {
    const { result } = renderHook(() => useCompletion());

    act(() => {
      result.current[1].autoTrigger('gi', { x: 24, y: 48 });
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

    expect(result.current[0].visible).toBe(false);
    expect(result.current[0].items).toHaveLength(0);
  });
});
