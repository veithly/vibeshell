import { useEffect, useRef, useState } from 'react';
import { MoreHorizontal } from 'lucide-react';
import { cn } from '../../lib/utils';
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
 * icon-button styling and a denser menu. High-frequency actions (Quick Cmd,
 * SFTP, split panes) stay as direct buttons in the parent toolbar; everything
 * else lives here.
 */
export function WorkspaceToolbar({ label, items, anyPressed = false }: WorkspaceToolbarProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };

    document.addEventListener('pointerdown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('pointerdown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

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
        <MoreHorizontal className="h-4 w-4" />
      </button>

      {open && (
        <div
          role="menu"
          aria-label={label}
          className="absolute right-0 top-full z-50 mt-1 w-56 overflow-hidden rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark py-1 shadow-xl"
        >
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              className={cn(
                'flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm text-tokyo-fg transition-colors',
                'hover:bg-tokyo-bg-hl focus:outline-none focus:ring-1 focus:ring-inset focus:ring-tokyo-blue',
                item.disabled && 'cursor-not-allowed opacity-50 hover:bg-transparent',
                item.pressed && 'bg-tokyo-selection'
              )}
              disabled={item.disabled}
              aria-pressed={item.pressed}
              onClick={() => run(item.onSelect)}
            >
              {item.icon}
              <span>{item.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
