export { Terminal } from './Terminal';
export type { TerminalHandle } from './Terminal';

// Completion exports
export { CompletionPopup } from './CompletionPopup';
export type { CompletionItem, CompletionType } from './CompletionPopup';
export { useCompletion } from './useCompletion';
export type { CompletionState, CompletionActions } from './useCompletion';
export {
  commonCommands,
  categoryInfo,
  getCommandSuggestions,
  getHistorySuggestions,
} from './completionData';
export type { CommandSuggestion, CommandCategory } from './completionData';
