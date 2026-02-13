import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { getCommandSuggestions, getHistorySuggestions } from './completionData';
import type { CompletionItem, CompletionType } from './CompletionPopup';

/**
 * Maximum number of commands to keep in history.
 */
const MAX_HISTORY_SIZE = 500;

/**
 * Storage key for persisting command history.
 */
const HISTORY_STORAGE_KEY = 'vibeshell_command_history';

/**
 * Minimum characters before auto-triggering completions.
 */
const AUTO_TRIGGER_MIN_CHARS = 1;


/**
 * Common environment variables.
 */
const COMMON_ENV_VARS = [
  { name: 'HOME', description: 'User home directory' },
  { name: 'USER', description: 'Current username' },
  { name: 'PATH', description: 'Executable search path' },
  { name: 'PWD', description: 'Present working directory' },
  { name: 'SHELL', description: 'Current shell' },
  { name: 'TERM', description: 'Terminal type' },
  { name: 'EDITOR', description: 'Default text editor' },
  { name: 'LANG', description: 'System language' },
  { name: 'LC_ALL', description: 'Locale setting' },
  { name: 'HOSTNAME', description: 'System hostname' },
  { name: 'DISPLAY', description: 'X display server' },
  { name: 'SSH_AUTH_SOCK', description: 'SSH agent socket' },
  { name: 'SSH_CLIENT', description: 'SSH client info' },
  { name: 'SSH_CONNECTION', description: 'SSH connection info' },
  { name: 'LOGNAME', description: 'Login name' },
  { name: 'TMPDIR', description: 'Temporary directory' },
  { name: 'XDG_CONFIG_HOME', description: 'XDG config directory' },
  { name: 'XDG_DATA_HOME', description: 'XDG data directory' },
  { name: 'XDG_CACHE_HOME', description: 'XDG cache directory' },
];

/**
 * Completion state returned by the useCompletion hook.
 */
export interface CompletionState {
  /** Whether the completion popup is visible */
  visible: boolean;
  /** Array of completion items to display */
  items: CompletionItem[];
  /** Currently selected item index */
  selectedIndex: number;
  /** Position of the popup (x, y coordinates) */
  position: { x: number; y: number };
  /** Current input line being completed */
  currentInput: string;
  /** The prefix being completed (for partial word completion) */
  completionPrefix: string;
  /** Ghost text suggestion (shown inline after cursor) */
  ghostText: string;
}

/**
 * Actions returned by the useCompletion hook.
 */
export interface CompletionActions {
  /** Show completions for the given input at the specified position */
  showCompletions: (input: string, position: { x: number; y: number }) => void;
  /** Hide the completion popup */
  hideCompletions: () => void;
  /** Select the next completion item */
  selectNext: () => void;
  /** Select the previous completion item */
  selectPrev: () => void;
  /** Set the selected index directly */
  setSelectedIndex: (index: number) => void;
  /** Get the currently selected completion item */
  getSelectedItem: () => CompletionItem | null;
  /** Add a command to history */
  addToHistory: (command: string) => void;
  /** Get current completion text to insert */
  getCompletionText: () => string | null;
  /** Update completions for current input */
  updateCompletions: (input: string) => void;
  /** Auto-trigger completions while typing */
  autoTrigger: (input: string, position: { x: number; y: number }) => void;
  /** Clear ghost text */
  clearGhostText: () => void;
  /** Get ghost text for the current input */
  getGhostText: () => string;
}

/**
 * Fuzzy match result with score and match ranges.
 */
interface FuzzyMatchResult {
  matches: boolean;
  score: number;
  ranges: Array<{ start: number; end: number }>;
}

/**
 * Perform fuzzy matching on a string.
 * Returns match status, score, and matched character ranges.
 */
function fuzzyMatch(pattern: string, text: string): FuzzyMatchResult {
  const patternLower = pattern.toLowerCase();
  const textLower = text.toLowerCase();

  // Exact prefix match gets highest score
  if (textLower.startsWith(patternLower)) {
    return {
      matches: true,
      score: 100 + (pattern.length / text.length) * 50,
      ranges: [{ start: 0, end: pattern.length }],
    };
  }

  // Check for contains match
  const containsIndex = textLower.indexOf(patternLower);
  if (containsIndex !== -1) {
    return {
      matches: true,
      score: 50 + (pattern.length / text.length) * 25,
      ranges: [{ start: containsIndex, end: containsIndex + pattern.length }],
    };
  }

  // Fuzzy matching - characters must appear in order
  const ranges: Array<{ start: number; end: number }> = [];
  let patternIndex = 0;
  let score = 0;
  let consecutiveMatches = 0;
  let lastMatchIndex = -1;

  for (let textIndex = 0; textIndex < textLower.length && patternIndex < patternLower.length; textIndex++) {
    if (textLower[textIndex] === patternLower[patternIndex]) {
      // Check if this is consecutive with last match
      if (lastMatchIndex === textIndex - 1) {
        consecutiveMatches++;
        // Extend the last range
        if (ranges.length > 0) {
          ranges[ranges.length - 1].end = textIndex + 1;
        }
      } else {
        consecutiveMatches = 1;
        ranges.push({ start: textIndex, end: textIndex + 1 });
      }

      // Score based on position and consecutiveness
      score += 10;
      score += consecutiveMatches * 5; // Bonus for consecutive matches
      if (textIndex === 0) score += 20; // Bonus for starting match
      if (textLower[textIndex - 1] === ' ' || textLower[textIndex - 1] === '-' || textLower[textIndex - 1] === '_') {
        score += 15; // Bonus for word boundary
      }

      lastMatchIndex = textIndex;
      patternIndex++;
    }
  }

  // All pattern characters must be found
  if (patternIndex === patternLower.length) {
    // Penalize long gaps
    const coverage = pattern.length / text.length;
    score = score * coverage;

    return {
      matches: true,
      score: Math.max(1, score),
      ranges,
    };
  }

  return { matches: false, score: 0, ranges: [] };
}

/**
 * Load command history from localStorage.
 */
function loadHistory(): string[] {
  try {
    const stored = localStorage.getItem(HISTORY_STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        return parsed.slice(-MAX_HISTORY_SIZE);
      }
    }
  } catch (error) {
    console.warn('Failed to load command history:', error);
  }
  return [];
}

/**
 * Save command history to localStorage.
 */
function saveHistory(history: string[]): void {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history.slice(-MAX_HISTORY_SIZE)));
  } catch (error) {
    console.warn('Failed to save command history:', error);
  }
}

/**
 * Get environment variable suggestions.
 */
function getEnvVarSuggestions(input: string, maxResults = 5): CompletionItem[] {
  // Check if user is typing an env var (starts with $ or after $)
  const envMatch = input.match(/\$([A-Za-z_]*)$/);
  if (!envMatch) return [];

  const prefix = envMatch[1].toLowerCase();

  const matches = COMMON_ENV_VARS
    .filter(env => env.name.toLowerCase().startsWith(prefix))
    .slice(0, maxResults)
    .map(env => ({
      text: `$${env.name}`,
      description: env.description,
      type: 'variable' as CompletionType,
      isHistory: false,
    }));

  return matches;
}

/**
 * Hook for managing terminal command completion.
 *
 * @returns Tuple of [state, actions]
 */
export function useCompletion(): [CompletionState, CompletionActions] {
  // State
  const [visible, setVisible] = useState(false);
  const [items, setItems] = useState<CompletionItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [currentInput, setCurrentInput] = useState('');
  const [completionPrefix, setCompletionPrefix] = useState('');
  const [ghostText, setGhostText] = useState('');

  // Refs
  const historyRef = useRef<string[]>(loadHistory());

  // Load history on mount
  useEffect(() => {
    historyRef.current = loadHistory();
  }, []);

  /**
   * Generate completion items from command suggestions and history.
   */
  const generateCompletions = useCallback((input: string): CompletionItem[] => {
    const trimmedInput = input.trim();
    if (!trimmedInput) {
      return [];
    }

    const results: Array<CompletionItem & { score: number }> = [];

    // Check for environment variable completion
    const envItems = getEnvVarSuggestions(trimmedInput);
    if (envItems.length > 0) {
      return envItems;
    }

    // Get command suggestions with fuzzy matching
    const commandSuggestions = getCommandSuggestions(trimmedInput, 15);
    commandSuggestions.forEach(cmd => {
      const fuzzyResult = fuzzyMatch(trimmedInput, cmd.text);
      if (fuzzyResult.matches) {
        results.push({
          text: cmd.text,
          description: cmd.description,
          category: cmd.category,
          isHistory: false,
          type: 'command' as CompletionType,
          score: fuzzyResult.score,
          matchRanges: fuzzyResult.ranges,
        });
      }
    });

    // Get history suggestions with fuzzy matching
    const historySuggestions = getHistorySuggestions(historyRef.current, trimmedInput, 10);
    historySuggestions.forEach(text => {
      const fuzzyResult = fuzzyMatch(trimmedInput, text);
      if (fuzzyResult.matches) {
        // Check if this is already in commands to avoid duplicates
        const isDuplicate = results.some(
          r => r.text.toLowerCase() === text.toLowerCase() && !r.isHistory
        );
        if (!isDuplicate) {
          results.push({
            text,
            description: 'From history',
            isHistory: true,
            type: 'history' as CompletionType,
            score: fuzzyResult.score + 10, // Slight boost for history
            matchRanges: fuzzyResult.ranges,
          });
        }
      }
    });

    // Sort by score descending
    results.sort((a, b) => b.score - a.score);

    // Remove score from final results and limit
    return results.slice(0, 12).map(({ score: _score, ...item }) => item);
  }, []);

  /**
   * Calculate ghost text suggestion based on input.
   */
  const calculateGhostText = useCallback((input: string): string => {
    const trimmedInput = input.trim();
    if (!trimmedInput || trimmedInput.length < 2) {
      return '';
    }

    // Check history first for most recent matching command
    const historyMatch = historyRef.current
      .slice()
      .reverse()
      .find(cmd => cmd.toLowerCase().startsWith(trimmedInput.toLowerCase()));

    if (historyMatch) {
      // Return the part of the command that comes after the current input
      return historyMatch.slice(trimmedInput.length);
    }

    // Fallback to command suggestions
    const suggestions = getCommandSuggestions(trimmedInput, 1);
    if (suggestions.length > 0) {
      const suggestion = suggestions[0].text;
      if (suggestion.toLowerCase().startsWith(trimmedInput.toLowerCase())) {
        return suggestion.slice(trimmedInput.length);
      }
    }

    return '';
  }, []);

  /**
   * Show completions for the given input.
   */
  const showCompletions = useCallback((input: string, pos: { x: number; y: number }) => {
    const completions = generateCompletions(input);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setPosition(pos);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());
      setVisible(true);

      // Calculate ghost text
      const ghost = calculateGhostText(input);
      setGhostText(ghost);
    } else {
      setVisible(false);
      setGhostText('');
    }
  }, [generateCompletions, calculateGhostText]);

  /**
   * Auto-trigger suggestions while typing (VS Code-style).
   */
  const autoTrigger = useCallback((input: string, pos: { x: number; y: number }) => {
    const trimmedInput = input.trim();

    // Don't suggest when input is too short
    if (trimmedInput.length < AUTO_TRIGGER_MIN_CHARS) {
      setVisible(false);
      setItems([]);
      setGhostText('');
      return;
    }

    // Generate completions and ghost text immediately (no debounce)
    // to ensure the popup appears reliably while typing
    const completions = generateCompletions(input);
    const ghost = calculateGhostText(input);

    console.debug('[Completion] autoTrigger:', { input: trimmedInput, completionsCount: completions.length, pos });

    setCurrentInput(input);
    setCompletionPrefix(trimmedInput);
    setGhostText(ghost);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setPosition(pos);
      setVisible(true);
    } else {
      setVisible(false);
      setItems([]);
    }
  }, [generateCompletions, calculateGhostText]);

  /**
   * Update completions for the current input without changing position.
   */
  const updateCompletions = useCallback((input: string) => {
    const completions = generateCompletions(input);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());

      // Update ghost text
      const ghost = calculateGhostText(input);
      setGhostText(ghost);
    } else {
      setVisible(false);
      setGhostText('');
    }
  }, [generateCompletions, calculateGhostText]);

  /**
   * Hide the completion popup.
   */
  const hideCompletions = useCallback(() => {
    setVisible(false);
    setItems([]);
    setSelectedIndex(0);
    setGhostText('');
  }, []);

  /**
   * Clear ghost text only.
   */
  const clearGhostText = useCallback(() => {
    setGhostText('');
  }, []);

  /**
   * Get current ghost text.
   */
  const getGhostText = useCallback(() => {
    return ghostText;
  }, [ghostText]);

  /**
   * Select the next completion item.
   */
  const selectNext = useCallback(() => {
    if (items.length > 0) {
      setSelectedIndex(prev => (prev + 1) % items.length);
    }
  }, [items.length]);

  /**
   * Select the previous completion item.
   */
  const selectPrev = useCallback(() => {
    if (items.length > 0) {
      setSelectedIndex(prev => (prev === 0 ? items.length - 1 : prev - 1));
    }
  }, [items.length]);

  /**
   * Get the currently selected completion item.
   */
  const getSelectedItem = useCallback((): CompletionItem | null => {
    if (visible && items.length > 0 && selectedIndex < items.length) {
      return items[selectedIndex];
    }
    return null;
  }, [visible, items, selectedIndex]);

  /**
   * Get the text to insert for the current completion.
   */
  const getCompletionText = useCallback((): string | null => {
    const item = getSelectedItem();
    if (!item) return null;

    // For history items, return the full command
    if (item.isHistory) {
      return item.text;
    }

    // For environment variables, return as-is
    if (item.type === 'variable') {
      return item.text;
    }

    // For command completions, check if we're completing a subcommand
    const parts = completionPrefix.split(/\s+/);
    if (parts.length >= 2) {
      // We're completing a subcommand - just return the subcommand part
      return item.text;
    }

    // Return the full command
    return item.text;
  }, [getSelectedItem, completionPrefix]);

  /**
   * Add a command to history.
   */
  const addToHistory = useCallback((command: string) => {
    const trimmed = command.trim();
    if (!trimmed) return;

    // Don't add if it's the same as the last command
    const lastCommand = historyRef.current[historyRef.current.length - 1];
    if (lastCommand === trimmed) return;

    historyRef.current = [...historyRef.current, trimmed].slice(-MAX_HISTORY_SIZE);
    saveHistory(historyRef.current);
  }, []);

  // Memoize state object to prevent unnecessary re-renders
  const state: CompletionState = useMemo(() => ({
    visible,
    items,
    selectedIndex,
    position,
    currentInput,
    completionPrefix,
    ghostText,
  }), [visible, items, selectedIndex, position, currentInput, completionPrefix, ghostText]);

  // Memoize actions object - actions are already stable via useCallback
  const actions: CompletionActions = useMemo(() => ({
    showCompletions,
    hideCompletions,
    selectNext,
    selectPrev,
    setSelectedIndex,
    getSelectedItem,
    addToHistory,
    getCompletionText,
    updateCompletions,
    autoTrigger,
    clearGhostText,
    getGhostText,
  }), [
    showCompletions,
    hideCompletions,
    selectNext,
    selectPrev,
    setSelectedIndex,
    getSelectedItem,
    addToHistory,
    getCompletionText,
    updateCompletions,
    autoTrigger,
    clearGhostText,
    getGhostText,
  ]);

  return [state, actions];
}
