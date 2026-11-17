import { useEffect, useRef, useCallback, useState, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../lib/utils';

export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
  /** Right-aligned keyboard hint rendered after the label (e.g. 'Ctrl+C'). */
  shortcut?: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
  divider?: boolean;
}

interface ContextMenuProps {
  isOpen: boolean;
  position: { x: number; y: number };
  items: ContextMenuItem[];
  onClose: () => void;
  /** Optional content rendered above the items, separated by a bottom border. */
  header?: React.ReactNode;
  /** Compact rows (py-1.5 instead of the default py-2). */
  dense?: boolean;
  /** Minimum menu width in px (default 160). */
  minWidth?: number;
  /** data-testid applied to the menu container. */
  testId?: string;
}

const VIEWPORT_MARGIN = 8;

/**
 * Generic context menu component.
 *
 * Owns presentation and dismissal: rendered in a portal on document.body,
 * clamped to the viewport, closed on outside mousedown/contextmenu and Escape.
 * Callers only describe their items.
 */
export function ContextMenu({
  isOpen,
  position,
  items,
  onClose,
  header,
  dense = false,
  minWidth = 160,
  testId,
}: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [adjustedPosition, setAdjustedPosition] = useState(position);

  // Close on click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      document.addEventListener('contextmenu', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('contextmenu', handleClickOutside);
    };
  }, [isOpen, onClose]);

  // Close on escape
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleEscape);
    }

    return () => {
      document.removeEventListener('keydown', handleEscape);
    };
  }, [isOpen, onClose]);

  // Adjust position to keep the menu within the viewport (measured after
  // layout so the real menu size is used; re-clamped on window resize).
  useLayoutEffect(() => {
    if (!isOpen) return;

    const fitMenuToViewport = () => {
      const menu = menuRef.current;
      if (!menu) return;

      const rect = menu.getBoundingClientRect();
      const maxX = Math.max(VIEWPORT_MARGIN, window.innerWidth - rect.width - VIEWPORT_MARGIN);
      const maxY = Math.max(VIEWPORT_MARGIN, window.innerHeight - rect.height - VIEWPORT_MARGIN);
      const x = Math.min(Math.max(VIEWPORT_MARGIN, position.x), maxX);
      const y = Math.min(Math.max(VIEWPORT_MARGIN, position.y), maxY);

      setAdjustedPosition((previous) => (
        previous.x === x && previous.y === y ? previous : { x, y }
      ));
    };

    fitMenuToViewport();
    window.addEventListener('resize', fitMenuToViewport);
    return () => window.removeEventListener('resize', fitMenuToViewport);
  }, [isOpen, position]);

  const handleItemClick = useCallback((item: ContextMenuItem) => {
    if (!item.disabled) {
      item.onClick();
      onClose();
    }
  }, [onClose]);

  if (!isOpen) return null;

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      data-testid={testId}
      className={cn(
        'fixed z-[100] py-1',
        'bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl',
        'overflow-y-auto overscroll-contain',
        'animate-fade-in'
      )}
      style={{
        left: adjustedPosition.x,
        top: adjustedPosition.y,
        minWidth,
        maxHeight: 'calc(100vh - 16px)',
      }}
    >
      {header && (
        <div className="px-3 py-1.5 text-xs text-tokyo-comment border-b border-tokyo-bg-hl mb-1 font-medium">
          {header}
        </div>
      )}
      {items.map((item, index) => {
        if (item.divider) {
          return (
            <div
              key={`divider-${index}`}
              className="my-1 border-t border-tokyo-bg-hl"
            />
          );
        }

        return (
          <button
            key={item.id}
            role="menuitem"
            className={cn(
              'w-full flex items-center gap-2 px-3 text-sm text-left',
              dense ? 'py-1.5' : 'py-2',
              'transition-colors duration-100',
              item.disabled
                ? 'text-tokyo-comment cursor-not-allowed opacity-50'
                : item.danger
                  ? 'text-tokyo-red hover:bg-tokyo-red/20'
                  : 'text-tokyo-fg hover:bg-tokyo-bg-hl hover:text-tokyo-fg'
            )}
            onClick={() => handleItemClick(item)}
            disabled={item.disabled}
          >
            {item.icon && <span className="w-4 h-4 flex-shrink-0">{item.icon}</span>}
            {item.label}
            {item.shortcut && (
              <span className="ml-auto text-xs text-tokyo-comment">{item.shortcut}</span>
            )}
          </button>
        );
      })}
    </div>,
    document.body
  );
}

export type { ContextMenuProps };
