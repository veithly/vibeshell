import { useEffect, useRef, useState } from 'react';
import { Check, MoreHorizontal } from 'lucide-react';
import { cn } from '../../lib/utils';
import { focusMenuItem, handleMenuNavigation } from '../../lib/menuNavigation';
import type { MobileWorkspaceMenuItem } from '../MobileWorkspaceActions/MobileWorkspaceActions';

export type WorkspaceMenuItem = MobileWorkspaceMenuItem;

interface WorkspaceToolbarProps {
  /** Accessible label / tooltip for the overflow trigger button. */
  label: string;
  items: WorkspaceMenuItem[];
  /** Whether any item is currently active, to highlight the trigger. */
  anyPressed?: boolean;
}

/**
 * Desktop overflow menu for low-frequency workspace actions. Mirrors the
 * dropdown pattern from MobileWorkspaceActions (pointerdown-outside close,
 * Escape close, focus restored to the trigger) but uses desktop-sized
 * icon-button styling and a denser menu. History remains directly accessible; other workspace actions live here.
 */
export function WorkspaceToolbar({ label, items, anyPressed = false }: WorkspaceToolbarProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setOpen(false); triggerRef.current?.focus(); }
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  useEffect(() => { if (open) focusMenuItem(menuRef.current); }, [open]);

  const run = (action: () => void) => {
    triggerRef.current?.focus();
    setOpen(false);
    action();
  };

  return (
    <div ref={rootRef} className="relative flex flex-shrink-0 items-center">
      <button
        ref={triggerRef}
        type="button"
        className={cn('icon-button tooltip-button', (open || anyPressed) && 'is-active')}
        data-tooltip={label}
        onClick={() => setOpen((current) => !current)}
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="menu"
      >
        <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
      </button>

      {open && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={label}
          className="glass-menu glass-menu-popover absolute right-0 top-full z-50 mt-2 w-60"
          onKeyDown={handleMenuNavigation}
        >
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              className={cn(
                'glass-menu-item',
                item.disabled && 'cursor-not-allowed opacity-50 hover:bg-transparent',
                item.pressed && 'bg-tokyo-selection'
              )}
              disabled={item.disabled}
              aria-pressed={item.pressed}
              onClick={() => run(item.onSelect)}
            >
              <span className="glass-menu-icon">{item.icon}</span>
              <span className="glass-menu-label">{item.label}</span>
              {item.pressed && <Check className="glass-menu-check" aria-hidden="true" />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
