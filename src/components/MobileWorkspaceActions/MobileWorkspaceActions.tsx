import { useEffect, useRef, useState, type ReactNode } from 'react';
import { FolderOpen, MoreHorizontal } from 'lucide-react';
import { cn } from '../../lib/utils';

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
          role="menu"
          className="absolute right-0 top-full z-50 mt-1 w-56 overflow-hidden rounded-md border border-tokyo-bg-hl bg-tokyo-bg-dark py-1 shadow-xl"
        >
          {menuItems.map((item) => (
            <button
              key={item.id}
              role="menuitem"
              className={cn('mobile-workspace-menu-item', item.pressed && 'bg-tokyo-selection')}
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
