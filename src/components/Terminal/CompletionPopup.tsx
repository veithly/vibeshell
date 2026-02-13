import { useEffect, useRef, memo } from 'react';
import { createPortal } from 'react-dom';
import { cn } from '../../lib/utils';
import { useSettingsStore, themes } from '../../stores/settingsStore';
import { categoryInfo, type CommandCategory } from './completionData';

/**
 * Type of completion item for icon display
 */
export type CompletionType = 'command' | 'file' | 'directory' | 'history' | 'variable' | 'subcommand';

/**
 * A single completion item that can be displayed in the popup.
 */
export interface CompletionItem {
  /** The text to insert when this item is selected */
  text: string;
  /** Optional description of the item */
  description?: string;
  /** Category for visual grouping */
  category?: CommandCategory;
  /** Whether this is from command history */
  isHistory?: boolean;
  /** Type of completion for icon display */
  type?: CompletionType;
  /** Match ranges for highlighting (fuzzy match) */
  matchRanges?: Array<{ start: number; end: number }>;
  /** File extension for file items */
  fileExtension?: string;
}

interface CompletionPopupProps {
  /** Array of completion items to display */
  items: CompletionItem[];
  /** Currently selected index */
  selectedIndex: number;
  /** Callback when an item is selected */
  onSelect: (item: CompletionItem) => void;
  /** Callback when selection changes via keyboard */
  onSelectionChange: (index: number) => void;
  /** Position relative to cursor (x, y coordinates) */
  position: { x: number; y: number };
  /** Whether the popup is visible */
  visible: boolean;
  /** Callback to close the popup */
  onClose: () => void;
  /** Current input for highlighting */
  currentInput?: string;
}

/**
 * Icon component for different completion types
 */
const CompletionIcon = memo(function CompletionIcon({
  type,
  category,
  isHistory,
  fileExtension,
  themeColors,
}: {
  type?: CompletionType;
  category?: CommandCategory;
  isHistory?: boolean;
  fileExtension?: string;
  themeColors: { fg: string; fgDark: string; accent: string };
}) {
  // History icon
  if (isHistory || type === 'history') {
    return (
      <svg
        className="w-4 h-4 flex-shrink-0"
        viewBox="0 0 16 16"
        fill="none"
        style={{ color: themeColors.fgDark }}
      >
        <path
          d="M8 3.5V8l3 1.5"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <circle
          cx="8"
          cy="8"
          r="5.5"
          stroke="currentColor"
          strokeWidth="1.5"
        />
        <path
          d="M3 1L1 3"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      </svg>
    );
  }

  // Directory icon
  if (type === 'directory') {
    return (
      <svg
        className="w-4 h-4 flex-shrink-0"
        viewBox="0 0 16 16"
        fill="none"
        style={{ color: '#e0af68' }}
      >
        <path
          d="M2 4.5A1.5 1.5 0 0 1 3.5 3H6l1 2h5.5A1.5 1.5 0 0 1 14 6.5v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5v-8z"
          fill="currentColor"
        />
      </svg>
    );
  }

  // File icon with extension-based coloring
  if (type === 'file') {
    const getFileColor = () => {
      if (!fileExtension) return themeColors.fgDark;
      const ext = fileExtension.toLowerCase();
      // Color by extension type
      if (['.ts', '.tsx', '.js', '.jsx'].includes(ext)) return '#7aa2f7';
      if (['.py'].includes(ext)) return '#9ece6a';
      if (['.rs'].includes(ext)) return '#ff9e64';
      if (['.go'].includes(ext)) return '#7dcfff';
      if (['.json', '.yaml', '.yml', '.toml'].includes(ext)) return '#e0af68';
      if (['.md', '.txt', '.doc'].includes(ext)) return themeColors.fg;
      if (['.css', '.scss', '.less'].includes(ext)) return '#bb9af7';
      if (['.html', '.htm'].includes(ext)) return '#f7768e';
      if (['.sh', '.bash', '.zsh'].includes(ext)) return '#9ece6a';
      return themeColors.fgDark;
    };

    return (
      <svg
        className="w-4 h-4 flex-shrink-0"
        viewBox="0 0 16 16"
        fill="none"
        style={{ color: getFileColor() }}
      >
        <path
          d="M4 2h5l4 4v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z"
          stroke="currentColor"
          strokeWidth="1.3"
        />
        <path d="M9 2v4h4" stroke="currentColor" strokeWidth="1.3" />
      </svg>
    );
  }

  // Variable icon
  if (type === 'variable') {
    return (
      <svg
        className="w-4 h-4 flex-shrink-0"
        viewBox="0 0 16 16"
        fill="none"
        style={{ color: '#bb9af7' }}
      >
        <text
          x="8"
          y="12"
          textAnchor="middle"
          fontSize="11"
          fontWeight="bold"
          fill="currentColor"
        >
          $
        </text>
      </svg>
    );
  }

  // Subcommand icon
  if (type === 'subcommand') {
    return (
      <svg
        className="w-4 h-4 flex-shrink-0"
        viewBox="0 0 16 16"
        fill="none"
        style={{ color: themeColors.accent }}
      >
        <path
          d="M5 4l4 4-4 4"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d="M9 12h4"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      </svg>
    );
  }

  // Default command icon with category coloring
  const categoryColor = category ? categoryInfo[category]?.color : themeColors.accent;

  return (
    <svg
      className="w-4 h-4 flex-shrink-0"
      viewBox="0 0 16 16"
      fill="none"
      style={{ color: categoryColor }}
    >
      <path
        d="M2 4l4 4-4 4"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M8 12h6"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
});

/**
 * Highlight matched characters in text
 */
const HighlightedText = memo(function HighlightedText({
  text,
  matchRanges,
  input,
  themeColors,
}: {
  text: string;
  matchRanges?: Array<{ start: number; end: number }>;
  input?: string;
  themeColors: { fg: string; fgDark: string; accent: string };
}) {
  // If we have explicit match ranges, use them
  if (matchRanges && matchRanges.length > 0) {
    const parts: JSX.Element[] = [];
    let lastIndex = 0;

    matchRanges.forEach((range, i) => {
      // Add non-matching part before this range
      if (range.start > lastIndex) {
        parts.push(
          <span key={`pre-${i}`}>{text.slice(lastIndex, range.start)}</span>
        );
      }
      // Add matching part
      parts.push(
        <span
          key={`match-${i}`}
          style={{ color: themeColors.accent, fontWeight: 600 }}
        >
          {text.slice(range.start, range.end)}
        </span>
      );
      lastIndex = range.end;
    });

    // Add remaining part
    if (lastIndex < text.length) {
      parts.push(<span key="end">{text.slice(lastIndex)}</span>);
    }

    return <>{parts}</>;
  }

  // Fallback: highlight based on input prefix match
  if (input) {
    const inputLower = input.toLowerCase().trim();
    const textLower = text.toLowerCase();
    const index = textLower.indexOf(inputLower);

    if (index !== -1) {
      return (
        <>
          {text.slice(0, index)}
          <span style={{ color: themeColors.accent, fontWeight: 600 }}>
            {text.slice(index, index + inputLower.length)}
          </span>
          {text.slice(index + inputLower.length)}
        </>
      );
    }
  }

  return <>{text}</>;
});

/**
 * CompletionPopup displays command completion suggestions near the cursor.
 * It supports keyboard navigation and mouse selection with VS Code-style UI.
 */
export const CompletionPopup = memo(function CompletionPopup({
  items,
  selectedIndex,
  onSelect,
  onSelectionChange,
  position,
  visible,
  onClose,
  currentInput,
}: CompletionPopupProps) {
  const popupRef = useRef<HTMLDivElement>(null);
  const selectedRef = useRef<HTMLDivElement>(null);
  const { settings } = useSettingsStore();

  // Get current theme colors
  const currentTheme = themes.find(t => t.name === settings.appearance.theme);
  const themeColors = currentTheme?.colors || themes[0].colors;

  // Scroll selected item into view
  useEffect(() => {
    if (selectedRef.current && popupRef.current) {
      selectedRef.current.scrollIntoView({
        block: 'nearest',
        behavior: 'smooth',
      });
    }
  }, [selectedIndex]);

  // Keyboard events are handled in Terminal.tsx's onData handler to avoid
  // conflicts with xterm.js input processing. This component handles only
  // mouse interaction (click to select, click outside to close).

  // Handle click outside to close
  useEffect(() => {
    if (!visible) return;

    const handleClickOutside = (event: MouseEvent) => {
      if (popupRef.current && !popupRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [visible, onClose]);

  if (!visible || items.length === 0) {
    return null;
  }

  // Calculate popup position to stay within viewport
  const maxWidth = 400;
  const maxHeight = 300;
  let left = position.x;
  let top = position.y;

  // Ensure popup stays within viewport bounds
  if (typeof window !== 'undefined') {
    if (left + maxWidth > window.innerWidth) {
      left = window.innerWidth - maxWidth - 10;
    }
    if (left < 10) {
      left = 10;
    }
    if (top + maxHeight > window.innerHeight) {
      // Position above cursor if not enough space below
      top = Math.max(10, position.y - maxHeight - 20);
    }
  }

  // Render in a portal to document.body to avoid any CSS stacking/overflow issues
  return createPortal(
    <div
      ref={popupRef}
      className={cn(
        'fixed overflow-hidden rounded-lg shadow-2xl',
        'border border-opacity-40'
      )}
      style={{
        left: `${left}px`,
        top: `${top}px`,
        minWidth: '200px',
        maxWidth: `${maxWidth}px`,
        maxHeight: `${maxHeight}px`,
        zIndex: 99999,
        backgroundColor: themeColors.bgDark,
        borderColor: themeColors.bgHl,
        boxShadow: `0 8px 32px rgba(0, 0, 0, 0.4), 0 0 0 1px ${themeColors.bgHl}`,
      }}
    >
      {/* Header */}
      <div
        className="px-3 py-1.5 text-xs font-medium border-b flex items-center justify-between"
        style={{
          backgroundColor: themeColors.bg,
          borderColor: themeColors.bgHl,
          color: themeColors.fgDark,
        }}
      >
        <span className="flex items-center gap-1.5">
          <svg className="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
            <path
              d="M2 4l4 4-4 4M8 12h6"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          Suggestions
        </span>
        <span className="opacity-60 text-xs tabular-nums">
          {items.length} {items.length === 1 ? 'item' : 'items'}
        </span>
      </div>

      {/* Items list */}
      <div
        className="overflow-y-auto overflow-x-hidden py-1"
        style={{ maxHeight: `${maxHeight - 64}px` }}
      >
        {items.map((item, index) => {
          const isSelected = index === selectedIndex;

          return (
            <div
              key={`${item.text}-${index}`}
              ref={isSelected ? selectedRef : undefined}
              className={cn(
                'flex items-center gap-2.5 px-3 py-1.5 cursor-pointer mx-1 rounded-md',
                'transition-all duration-75'
              )}
              style={{
                backgroundColor: isSelected ? themeColors.bgHl : 'transparent',
                transform: isSelected ? 'scale(1)' : 'scale(1)',
              }}
              onClick={() => onSelect(item)}
              onMouseEnter={() => onSelectionChange(index)}
            >
              {/* Icon */}
              <CompletionIcon
                type={item.type}
                category={item.category}
                isHistory={item.isHistory}
                fileExtension={item.fileExtension}
                themeColors={themeColors}
              />

              {/* Content */}
              <div className="flex-1 min-w-0 overflow-hidden">
                <div className="flex items-center gap-2">
                  <span
                    className="font-mono text-sm truncate"
                    style={{ color: themeColors.fg }}
                  >
                    <HighlightedText
                      text={item.text}
                      matchRanges={item.matchRanges}
                      input={currentInput}
                      themeColors={themeColors}
                    />
                  </span>
                  {item.isHistory && (
                    <span
                      className="text-xs px-1.5 py-0.5 rounded"
                      style={{
                        backgroundColor: `${themeColors.fgDark}20`,
                        color: themeColors.fgDark,
                      }}
                    >
                      history
                    </span>
                  )}
                </div>
                {item.description && (
                  <div
                    className="text-xs truncate mt-0.5"
                    style={{ color: themeColors.fgDark }}
                  >
                    {item.description}
                  </div>
                )}
              </div>

              {/* Category badge */}
              {item.category && !item.isHistory && (
                <div
                  className="text-xs px-1.5 py-0.5 rounded flex-shrink-0"
                  style={{
                    backgroundColor: `${categoryInfo[item.category]?.color || themeColors.accent}15`,
                    color: categoryInfo[item.category]?.color || themeColors.accent,
                  }}
                >
                  {categoryInfo[item.category]?.label || item.category}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Footer with keyboard hints */}
      <div
        className="px-3 py-1.5 text-xs border-t flex gap-4 items-center"
        style={{
          backgroundColor: themeColors.bg,
          borderColor: themeColors.bgHl,
          color: themeColors.fgDark,
        }}
      >
        <span className="flex items-center gap-1">
          <kbd
            className="px-1.5 py-0.5 rounded text-xs font-mono"
            style={{ backgroundColor: themeColors.bgHl }}
          >
            Tab
          </kbd>
          <span className="opacity-80">Accept</span>
        </span>
        <span className="flex items-center gap-1">
          <kbd
            className="px-1 py-0.5 rounded text-xs font-mono"
            style={{ backgroundColor: themeColors.bgHl }}
          >
            {'\u2191\u2193'}
          </kbd>
          <span className="opacity-80">Navigate</span>
        </span>
        <span className="flex items-center gap-1">
          <kbd
            className="px-1.5 py-0.5 rounded text-xs font-mono"
            style={{ backgroundColor: themeColors.bgHl }}
          >
            Esc
          </kbd>
          <span className="opacity-80">Dismiss</span>
        </span>
      </div>
    </div>,
    document.body
  );
});
