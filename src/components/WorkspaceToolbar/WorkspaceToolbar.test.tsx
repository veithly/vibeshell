import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WorkspaceToolbar } from './WorkspaceToolbar';

afterEach(cleanup);

describe('WorkspaceToolbar', () => {
  it('opens the menu on trigger click and invokes the selected item', () => {
    const onSnippets = vi.fn();
    render(
      <WorkspaceToolbar
        label="More actions"
        items={[
          { id: 'snippets', label: 'Snippets', icon: <span aria-hidden="true">S</span>, onSelect: onSnippets },
        ]}
      />
    );

    expect(screen.queryByRole('menu')).not.toBeInTheDocument();

    const trigger = screen.getByRole('button', { name: 'More actions' });
    fireEvent.click(trigger);
    expect(screen.getByRole('menu')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('menuitem', { name: 'Snippets' }));
    expect(onSnippets).toHaveBeenCalledOnce();
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it('closes on Escape without invoking any item', () => {
    const onSelect = vi.fn();
    render(
      <WorkspaceToolbar
        label="More actions"
        items={[{ id: 'settings', label: 'Settings', icon: <span />, onSelect }]}
      />
    );

    const trigger = screen.getByRole('button', { name: 'More actions' });
    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('closes when a pointerdown lands outside the menu', () => {
    render(
      <WorkspaceToolbar
        label="More actions"
        items={[{ id: 'x', label: 'X', icon: <span />, onSelect: vi.fn() }]}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));
    expect(screen.getByRole('menu')).toBeInTheDocument();

    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  });

  it('respects disabled and pressed item states', () => {
    const onDisabled = vi.fn();
    const onToggled = vi.fn();
    render(
      <WorkspaceToolbar
        label="More actions"
        items={[
          { id: 'disabled', label: 'Disabled', icon: <span />, disabled: true, onSelect: onDisabled },
          { id: 'toggled', label: 'Toggled', icon: <span />, pressed: true, onSelect: onToggled },
        ]}
      />
    );

    fireEvent.click(screen.getByRole('button', { name: 'More actions' }));

    const disabledItem = screen.getByRole('menuitem', { name: 'Disabled' });
    expect(disabledItem).toBeDisabled();

    const toggledItem = screen.getByRole('menuitem', { name: 'Toggled' });
    expect(toggledItem).toHaveAttribute('aria-pressed', 'true');
  });

  it('reflects anyPressed on the trigger for quick scanning', () => {
    const { rerender } = render(
      <WorkspaceToolbar label="More actions" items={[]} anyPressed={false} />
    );
    expect(screen.getByRole('button', { name: 'More actions' })).not.toHaveClass('is-active');

    rerender(<WorkspaceToolbar label="More actions" items={[]} anyPressed={true} />);
    expect(screen.getByRole('button', { name: 'More actions' })).toHaveClass('is-active');
  });
});
