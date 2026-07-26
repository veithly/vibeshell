import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CompletionPopup } from './CompletionPopup';

describe('CompletionPopup mobile layout', () => {
  beforeEach(() => {
    Object.defineProperty(window, 'innerWidth', { value: 320, configurable: true });
    Object.defineProperty(window, 'innerHeight', { value: 640, configurable: true });
    Element.prototype.scrollIntoView = vi.fn();
  });

  afterEach(cleanup);

  it('clamps its width and horizontal position to a narrow viewport', () => {
    render(
      <CompletionPopup
        items={[{ text: 'git status', description: 'Show working tree status' }]}
        selectedIndex={0}
        onSelect={vi.fn()}
        onSelectionChange={vi.fn()}
        position={{ x: 280, y: 80 }}
        visible
        onClose={vi.fn()}
      />
    );

    const popup = screen.getByRole('listbox', { name: 'Command suggestions' });

    expect(popup).toHaveStyle({
      left: '10px',
      width: '300px',
      minWidth: '200px',
    });
  });
});
