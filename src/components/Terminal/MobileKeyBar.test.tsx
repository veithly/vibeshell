import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MobileKeyBar } from './MobileKeyBar';

afterEach(cleanup);

describe('MobileKeyBar', () => {
  it('sends terminal control sequences without stealing focus on pointer down', () => {
    const onSend = vi.fn();
    render(<MobileKeyBar onSend={onSend} onPaste={vi.fn()} />);

    const escapeKey = screen.getByRole('button', { name: 'Esc' });
    const pointerDown = new Event('pointerdown', { bubbles: true, cancelable: true });
    escapeKey.dispatchEvent(pointerDown);
    fireEvent.click(escapeKey);

    expect(pointerDown.defaultPrevented).toBe(true);
    expect(onSend).toHaveBeenCalledWith('\x1b');
  });

  it('exposes paste as an explicit touch action', () => {
    const onPaste = vi.fn();
    render(<MobileKeyBar onSend={vi.fn()} onPaste={onPaste} />);

    fireEvent.click(screen.getByRole('button', { name: 'Paste' }));

    expect(onPaste).toHaveBeenCalledOnce();
  });
});
