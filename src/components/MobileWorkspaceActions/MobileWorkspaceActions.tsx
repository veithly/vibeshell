import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Check, FolderOpen, MoreHorizontal } from 'lucide-react';
import { cn } from '../../lib/utils';
import { focusMenuItem, handleMenuNavigation } from '../../lib/menuNavigation';

export interface MobileWorkspaceMenuItem {
  id: string;
  label: string;
  icon: ReactNode;
  disabled?: boolean;
  pressed?: boolean;
  onSelect: () => void;
}

interface MobileWorkspaceActionsProps {
  isSftpOpen: boolean;
  sftpDisabled: boolean;
  labels: {
    sftp: string;
    more: string;
  };
  menuItems: MobileWorkspaceMenuItem[];
  onToggleSftp: () => void;
}

export function MobileWorkspaceActions({
  isSftpOpen,
  sftpDisabled,
  labels,
  menuItems,
  onToggleSftp,
}: MobileWorkspaceActionsProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setOpen(false); moreButtonRef.current?.focus(); }
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
    moreButtonRef.current?.focus();
    setOpen(false);
    action();
  };

  return (
    <div ref={rootRef} className="relative flex flex-shrink-0 items-center gap-1 border-l border-tokyo-bg-hl pl-1">
      <button
        type="button"
        className={cn('icon-button h-11 w-11', isSftpOpen && 'is-active')}
        onClick={onToggleSftp}
        disabled={sftpDisabled}
        aria-label={labels.sftp}
        aria-pressed={isSftpOpen}
      >
        <FolderOpen className="h-4 w-4" />
      </button>
      <button
        ref={moreButtonRef}
        type="button"
        className={cn('icon-button h-11 w-11', open && 'is-active')}
        onClick={() => setOpen((current) => !current)}
        aria-label={labels.more}
        aria-expanded={open}
        aria-haspopup="menu"
      >
        <MoreHorizontal className="h-5 w-5" />
      </button>

      {open && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={labels.more}
          className="glass-menu glass-menu-popover absolute right-0 top-full z-50 mt-2 w-60"
          onKeyDown={handleMenuNavigation}
        >
          {menuItems.map((item) => (
            <button
              key={item.id}
              role="menuitem"
              className={cn('glass-menu-item mobile-glass-menu-item', item.pressed && 'bg-tokyo-selection')}
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
