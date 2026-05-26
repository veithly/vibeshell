import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { getCommandSuggestions, getHistorySuggestions } from './completionData';
import type { CompletionItem, CompletionType } from './CompletionPopup';

const MAX_HISTORY_SIZE = 500;
const HISTORY_STORAGE_KEY = 'vibeshell_command_history';
const AUTO_TRIGGER_MIN_CHARS = 1;

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

export interface CompletionState {
  visible: boolean;
  items: CompletionItem[];
  selectedIndex: number;
  position: { x: number; y: number };
  currentInput: string;
  completionPrefix: string;
  ghostText: string;
}

export interface CompletionActions {
  showCompletions: (input: string, position: { x: number; y: number }) => void;
  hideCompletions: () => void;
  selectNext: () => void;
  selectPrev: () => void;
  setSelectedIndex: (index: number) => void;
  getSelectedItem: () => CompletionItem | null;
  addToHistory: (command: string) => void;
  getCompletionText: () => string | null;
  updateCompletions: (input: string) => void;
  autoTrigger: (input: string, position: { x: number; y: number }) => void;
  clearGhostText: () => void;
  getGhostText: () => string;
}

interface FuzzyMatchResult {
  matches: boolean;
  score: number;
  ranges: Array<{ start: number; end: number }>;
}

function fuzzyMatch(pattern: string, text: string): FuzzyMatchResult {
  const patternLower = pattern.toLowerCase();
  const textLower = text.toLowerCase();

  if (textLower.startsWith(patternLower)) {
    return {
      matches: true,
      score: 100 + (pattern.length / text.length) * 50,
      ranges: [{ start: 0, end: pattern.length }],
    };
  }

  const containsIndex = textLower.indexOf(patternLower);
  if (containsIndex !== -1) {
    return {
      matches: true,
      score: 50 + (pattern.length / text.length) * 25,
      ranges: [{ start: containsIndex, end: containsIndex + pattern.length }],
    };
  }

  const ranges: Array<{ start: number; end: number }> = [];
  let patternIndex = 0;
  let score = 0;
  let consecutiveMatches = 0;
  let lastMatchIndex = -1;

  for (let textIndex = 0; textIndex < textLower.length && patternIndex < patternLower.length; textIndex++) {
    if (textLower[textIndex] === patternLower[patternIndex]) {
      if (lastMatchIndex === textIndex - 1) {
        consecutiveMatches++;
        if (ranges.length > 0) {
          ranges[ranges.length - 1].end = textIndex + 1;
        }
      } else {
        consecutiveMatches = 1;
        ranges.push({ start: textIndex, end: textIndex + 1 });
      }

      score += 10;
      score += consecutiveMatches * 5;
      if (textIndex === 0) score += 20;
      if (textLower[textIndex - 1] === ' ' || textLower[textIndex - 1] === '-' || textLower[textIndex - 1] === '_') {
        score += 15;
      }

      lastMatchIndex = textIndex;
      patternIndex++;
    }
  }

  if (patternIndex === patternLower.length) {
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

function saveHistory(history: string[]): void {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history.slice(-MAX_HISTORY_SIZE)));
  } catch (error) {
    console.warn('Failed to save command history:', error);
  }
}

function getEnvVarSuggestions(input: string, maxResults = 5): CompletionItem[] {
  const envMatch = input.match(/\$([A-Za-z_]*)$/);
  if (!envMatch) return [];

  const prefix = envMatch[1].toLowerCase();

  return COMMON_ENV_VARS
    .filter((env) => env.name.toLowerCase().startsWith(prefix))
    .slice(0, maxResults)
    .map((env) => ({
      text: `$${env.name}`,
      description: env.description,
      type: 'variable' as CompletionType,
      isHistory: false,
    }));
}

export function useCompletion(): [CompletionState, CompletionActions] {
  const [visible, setVisible] = useState(false);
  const [items, setItems] = useState<CompletionItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [currentInput, setCurrentInput] = useState('');
  const [completionPrefix, setCompletionPrefix] = useState('');
  const [ghostText, setGhostText] = useState('');

  const historyRef = useRef<string[]>(loadHistory());

  useEffect(() => {
    historyRef.current = loadHistory();
  }, []);

  const generateCompletions = useCallback((input: string): CompletionItem[] => {
    const trimmedInput = input.trim();
    if (!trimmedInput) {
      return [];
    }

    const results: Array<CompletionItem & { score: number }> = [];

    const envItems = getEnvVarSuggestions(trimmedInput);
    if (envItems.length > 0) {
      return envItems;
    }

    const commandSuggestions = getCommandSuggestions(trimmedInput, 15);
    commandSuggestions.forEach((cmd) => {
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

    const historySuggestions = getHistorySuggestions(historyRef.current, trimmedInput, 10);
    historySuggestions.forEach((text) => {
      const fuzzyResult = fuzzyMatch(trimmedInput, text);
      if (fuzzyResult.matches) {
        const isDuplicate = results.some(
          (r) => r.text.toLowerCase() === text.toLowerCase() && !r.isHistory
        );
        if (!isDuplicate) {
          results.push({
            text,
            description: 'From history',
            isHistory: true,
            type: 'history' as CompletionType,
            score: fuzzyResult.score + 10,
            matchRanges: fuzzyResult.ranges,
          });
        }
      }
    });

    results.sort((a, b) => b.score - a.score);
    return results.slice(0, 12).map(({ score: _score, ...item }) => item);
  }, []);

  const calculateGhostText = useCallback((input: string): string => {
    const trimmedInput = input.trim();
    if (!trimmedInput || trimmedInput.length < 2) {
      return '';
    }

    const historyMatch = historyRef.current
      .slice()
      .reverse()
      .find((cmd) => cmd.toLowerCase().startsWith(trimmedInput.toLowerCase()));

    if (historyMatch) {
      return historyMatch.slice(trimmedInput.length);
    }

    const suggestions = getCommandSuggestions(trimmedInput, 1);
    if (suggestions.length > 0) {
      const suggestion = suggestions[0].text;
      if (suggestion.toLowerCase().startsWith(trimmedInput.toLowerCase())) {
        return suggestion.slice(trimmedInput.length);
      }
    }

    return '';
  }, []);

  const showCompletions = useCallback((input: string, pos: { x: number; y: number }) => {
    const completions = generateCompletions(input);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setPosition(pos);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());
      setVisible(true);
      setGhostText(calculateGhostText(input));
    } else {
      setVisible(false);
      setGhostText('');
    }
  }, [generateCompletions, calculateGhostText]);

  const autoTrigger = useCallback((input: string, pos: { x: number; y: number }) => {
    const trimmedInput = input.trim();

    if (trimmedInput.length < AUTO_TRIGGER_MIN_CHARS) {
      setVisible(false);
      setItems([]);
      setGhostText('');
      return;
    }

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

  const updateCompletions = useCallback((input: string) => {
    const completions = generateCompletions(input);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());
      setGhostText(calculateGhostText(input));
    } else {
      setVisible(false);
      setGhostText('');
    }
  }, [generateCompletions, calculateGhostText]);

  const hideCompletions = useCallback(() => {
    setVisible(false);
    setItems([]);
    setSelectedIndex(0);
    setGhostText('');
  }, []);

  const clearGhostText = useCallback(() => {
    setGhostText('');
  }, []);

  const getGhostText = useCallback(() => ghostText, [ghostText]);

  const selectNext = useCallback(() => {
    if (items.length > 0) {
      setSelectedIndex((prev) => (prev + 1) % items.length);
    }
  }, [items.length]);

  const selectPrev = useCallback(() => {
    if (items.length > 0) {
      setSelectedIndex((prev) => (prev === 0 ? items.length - 1 : prev - 1));
    }
  }, [items.length]);

  const getSelectedItem = useCallback((): CompletionItem | null => {
    if (visible && items.length > 0 && selectedIndex < items.length) {
      return items[selectedIndex];
    }
    return null;
  }, [visible, items, selectedIndex]);

  const getCompletionText = useCallback((): string | null => {
    const item = getSelectedItem();
    if (!item) return null;

    if (item.isHistory || item.type === 'variable') {
      return item.text;
    }

    const parts = completionPrefix.split(/\s+/);
    if (parts.length >= 2) {
      return item.text;
    }

    return item.text;
  }, [getSelectedItem, completionPrefix]);

  const addToHistory = useCallback((command: string) => {
    const trimmed = command.trim();
    if (!trimmed) return;

    const lastCommand = historyRef.current[historyRef.current.length - 1];
    if (lastCommand === trimmed) return;

    historyRef.current = [...historyRef.current, trimmed].slice(-MAX_HISTORY_SIZE);
    saveHistory(historyRef.current);
  }, []);

  const state: CompletionState = useMemo(() => ({
    visible,
    items,
    selectedIndex,
    position,
    currentInput,
    completionPrefix,
    ghostText,
  }), [visible, items, selectedIndex, position, currentInput, completionPrefix, ghostText]);

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
