import { useEffect, useRef, useCallback } from 'react';
import { cn } from '../../lib/utils';

export interface ContextMenuItem {
  id: string;
  label: string;
  icon?: React.ReactNode;
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
}

/**
 * Generic context menu component
 */
export function ContextMenu({ isOpen, position, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

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

  // Adjust position to keep menu within viewport
  const getAdjustedPosition = useCallback(() => {
    if (!menuRef.current) return position;

    const menuRect = menuRef.current.getBoundingClientRect();
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;

    let x = position.x;
    let y = position.y;

    // Adjust horizontal position
    if (x + menuRect.width > viewportWidth) {
      x = viewportWidth - menuRect.width - 8;
    }

    // Adjust vertical position
    if (y + menuRect.height > viewportHeight) {
      y = viewportHeight - menuRect.height - 8;
    }

    return { x: Math.max(8, x), y: Math.max(8, y) };
  }, [position]);

  const handleItemClick = useCallback((item: ContextMenuItem) => {
    if (!item.disabled) {
      item.onClick();
      onClose();
    }
  }, [onClose]);

  if (!isOpen) return null;

  const adjustedPosition = getAdjustedPosition();

  return (
    <div
      ref={menuRef}
      className={cn(
        'fixed z-[100] min-w-[160px] py-1',
        'bg-tokyo-bg-dark border border-tokyo-bg-hl rounded-lg shadow-xl',
        'animate-fade-in'
      )}
      style={{
        left: adjustedPosition.x,
        top: adjustedPosition.y,
      }}
    >
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
            className={cn(
              'w-full flex items-center gap-2 px-3 py-2 text-sm text-left',
              'transition-colors duration-100',
              item.disabled
                ? 'text-tokyo-comment cursor-not-allowed opacity-50'
                : item.danger
                  ? 'text-tokyo-red hover:bg-tokyo-red/20'
                  : 'text-tokyo-fg hover:bg-tokyo-bg-hl hover:text-white'
            )}
            onClick={() => handleItemClick(item)}
            disabled={item.disabled}
          >
            {item.icon && <span className="w-4 h-4 flex-shrink-0">{item.icon}</span>}
            {item.label}
          </button>
        );
      })}
    </div>
  );
}

export type { ContextMenuProps };
