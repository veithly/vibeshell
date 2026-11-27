import { useEffect, useRef, useCallback, useState, useLayoutEffect } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../lib/utils';
import { focusMenuItem, handleMenuNavigation } from '../../lib/menuNavigation';

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
  const previousFocusRef = useRef<HTMLElement | null>(null);
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
        previousFocusRef.current?.focus();
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleEscape);
    }

    return () => {
      document.removeEventListener('keydown', handleEscape);
    };
  }, [isOpen, onClose]);

  useEffect(() => {
    if (!isOpen) return;
    previousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    focusMenuItem(menuRef.current);
  }, [isOpen]);

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
      onKeyDown={handleMenuNavigation}
      data-testid={testId}
      className={cn(
        'fixed z-[100] overflow-y-auto glass-menu glass-menu-popover',
        dense && 'glass-menu-dense'
      )}
      style={{
        left: adjustedPosition.x,
        top: adjustedPosition.y,
        minWidth: `min(${minWidth}px, calc(100vw - 16px))`,
        maxHeight: 'calc(100vh - 16px)',
      }}
    >
      {header && (
        <div className="glass-menu-heading">
          {header}
        </div>
      )}
      {items.map((item, index) => {
        if (item.divider) {
          return (
            <div
              key={`divider-${index}`}
              role="separator"
              className="glass-menu-separator"
            />
          );
        }

        return (
          <button
            key={item.id}
            role="menuitem"
            className={cn(
              'glass-menu-item',
              item.disabled
                ? 'text-tokyo-comment cursor-not-allowed opacity-50'
                : item.danger
                  ? 'glass-menu-danger text-tokyo-red'
                  : 'text-tokyo-fg hover:bg-tokyo-bg-hl hover:text-tokyo-fg'
            )}
            onClick={() => handleItemClick(item)}
            disabled={item.disabled}
          >
            {item.icon && <span className="glass-menu-icon">{item.icon}</span>}
            <span className="glass-menu-label">{item.label}</span>
            {item.shortcut && (
              <kbd className="glass-menu-shortcut">{item.shortcut}</kbd>
            )}
          </button>
        );
      })}
    </div>,
    document.body
  );
}

export type { ContextMenuProps };
