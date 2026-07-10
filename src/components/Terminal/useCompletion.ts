import { useState, useCallback, useRef, useEffect, useMemo } from 'react';
import { getCommandSuggestions, getCompletionQuery, getHistorySuggestions } from './completionData';
import type { CompletionItem, CompletionType } from './CompletionPopup';
import {
  isAiPredictionReady,
  predictCommandSuffix,
  type AiPredictionSettings,
} from '../../lib/aiCommandPrediction';

const MAX_HISTORY_SIZE = 500;
const HISTORY_STORAGE_KEY = 'vibeshell_command_history';
const AUTO_TRIGGER_MIN_CHARS = 2;

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
    const stored = globalThis.localStorage?.getItem(HISTORY_STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      if (Array.isArray(parsed)) {
        return parsed.slice(-MAX_HISTORY_SIZE);
      }
    }
  } catch {
    // Completion still works without persisted history.
  }
  return [];
}

function saveHistory(history: string[]): void {
  try {
    globalThis.localStorage?.setItem(HISTORY_STORAGE_KEY, JSON.stringify(history.slice(-MAX_HISTORY_SIZE)));
  } catch {
    // Completion still works without persisted history.
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

export function useCompletion(aiPredictionSettings?: AiPredictionSettings): [CompletionState, CompletionActions] {
  const [visible, setVisible] = useState(false);
  const [items, setItems] = useState<CompletionItem[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [currentInput, setCurrentInput] = useState('');
  const [completionPrefix, setCompletionPrefix] = useState('');
  const [ghostText, setGhostText] = useState('');

  const historyRef = useRef<string[]>(loadHistory());
  const aiPredictionSettingsRef = useRef(aiPredictionSettings);
  const aiPredictionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const aiPredictionAbortRef = useRef<AbortController | null>(null);
  const aiPredictionRequestIdRef = useRef(0);
  const latestInputRef = useRef('');

  useEffect(() => {
    historyRef.current = loadHistory();
  }, []);

  useEffect(() => {
    aiPredictionSettingsRef.current = aiPredictionSettings;
  }, [aiPredictionSettings]);

  const cancelAiPrediction = useCallback(() => {
    aiPredictionRequestIdRef.current++;

    if (aiPredictionTimerRef.current) {
      clearTimeout(aiPredictionTimerRef.current);
      aiPredictionTimerRef.current = null;
    }

    if (aiPredictionAbortRef.current) {
      aiPredictionAbortRef.current.abort();
      aiPredictionAbortRef.current = null;
    }
  }, []);

  useEffect(() => cancelAiPrediction, [cancelAiPrediction]);

  const generateCompletions = useCallback((input: string): CompletionItem[] => {
    const trimmedInput = input.trim();
    if (!trimmedInput) {
      return [];
    }

    const results: Array<CompletionItem & { score: number }> = [];
    const completionQuery = getCompletionQuery(input);

    const envItems = getEnvVarSuggestions(trimmedInput);
    if (envItems.length > 0) {
      return envItems;
    }

    const commandSuggestions = getCommandSuggestions(input, 15);
    commandSuggestions.forEach((cmd) => {
      const matchQuery = completionQuery;
      const fuzzyResult = matchQuery ? fuzzyMatch(matchQuery, cmd.text) : {
        matches: true,
        score: 1,
        ranges: [],
      };
      if (fuzzyResult.matches) {
        results.push({
          text: cmd.text,
          description: cmd.description,
          category: cmd.category,
          isHistory: false,
          type: (cmd.completionType ?? 'command') as CompletionType,
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

  const calculateLocalGhostText = useCallback((input: string): string => {
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

    const suggestions = getCommandSuggestions(input, 1);
    if (suggestions.length > 0) {
      const suggestion = suggestions[0].text;
      const completionQuery = getCompletionQuery(input);
      if (completionQuery && suggestion.toLowerCase().startsWith(completionQuery.toLowerCase())) {
        return suggestion.slice(completionQuery.length);
      }
    }

    return '';
  }, []);

  const scheduleAiPrediction = useCallback((input: string, localCompletions: CompletionItem[]) => {
    const settings = aiPredictionSettingsRef.current;
    latestInputRef.current = input;

    if (!settings || !isAiPredictionReady(settings) || input.trim().length < settings.minChars) {
      cancelAiPrediction();
      return;
    }

    cancelAiPrediction();

    const requestId = ++aiPredictionRequestIdRef.current;
    const controller = new AbortController();
    aiPredictionAbortRef.current = controller;

    aiPredictionTimerRef.current = setTimeout(async () => {
      aiPredictionTimerRef.current = null;

      try {
        const suffix = await predictCommandSuffix(
          settings,
          {
            input,
            history: historyRef.current,
            localSuggestions: localCompletions.map((item) => item.text),
          },
          controller.signal
        );

        if (
          requestId === aiPredictionRequestIdRef.current &&
          latestInputRef.current === input &&
          suffix
        ) {
          setGhostText(suffix);
        }
      } catch (error) {
        if (!(error instanceof DOMException && error.name === 'AbortError')) {
          console.debug('[Completion] AI prediction skipped:', error);
        }
      } finally {
        if (aiPredictionAbortRef.current === controller) {
          aiPredictionAbortRef.current = null;
        }
      }
    }, settings.debounceMs);
  }, [cancelAiPrediction]);

  const showCompletions = useCallback((input: string, pos: { x: number; y: number }) => {
    const completions = generateCompletions(input);
    const ghost = calculateLocalGhostText(input);
    latestInputRef.current = input;
    scheduleAiPrediction(input, completions);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setPosition(pos);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());
      setVisible(true);
      setGhostText(ghost);
    } else {
      setVisible(false);
      setGhostText(ghost);
    }
  }, [generateCompletions, calculateLocalGhostText, scheduleAiPrediction]);

  const autoTrigger = useCallback((input: string, pos: { x: number; y: number }) => {
    const trimmedInput = input.trim();

    if (trimmedInput.length < AUTO_TRIGGER_MIN_CHARS) {
      setVisible(false);
      setItems([]);
      setGhostText('');
      cancelAiPrediction();
      return;
    }

    const completions = generateCompletions(input);
    const ghost = calculateLocalGhostText(input);

    setCurrentInput(input);
    setCompletionPrefix(trimmedInput);
    setGhostText(ghost);
    setItems(completions);
    setSelectedIndex(0);
    setPosition(pos);
    setVisible(completions.length > 0);
    scheduleAiPrediction(input, completions);
  }, [generateCompletions, calculateLocalGhostText, scheduleAiPrediction, cancelAiPrediction]);

  const updateCompletions = useCallback((input: string) => {
    const completions = generateCompletions(input);
    const ghost = calculateLocalGhostText(input);
    latestInputRef.current = input;
    scheduleAiPrediction(input, completions);

    if (completions.length > 0) {
      setItems(completions);
      setSelectedIndex(0);
      setCurrentInput(input);
      setCompletionPrefix(input.trim());
      setGhostText(ghost);
    } else {
      setVisible(false);
      setItems([]);
      setGhostText(ghost);
    }
  }, [generateCompletions, calculateLocalGhostText, scheduleAiPrediction]);

  const hideCompletions = useCallback(() => {
    setVisible(false);
    setItems([]);
    setSelectedIndex(0);
    setGhostText('');
    cancelAiPrediction();
  }, [cancelAiPrediction]);

  const clearGhostText = useCallback(() => {
    setGhostText('');
    cancelAiPrediction();
  }, [cancelAiPrediction]);

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
