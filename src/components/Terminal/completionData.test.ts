import { describe, expect, it } from 'vitest';
import {
  getCommandCompletionContext,
  getCommandSuggestions,
} from './completionData';

describe('terminal command completion data', () => {
  it('suggests subcommands from the active command token', () => {
    const suggestions = getCommandSuggestions('git ch', 5).map((item) => item.text);

    expect(suggestions).toContain('checkout');
    expect(suggestions).toContain('cherry-pick');
    expect(suggestions).not.toContain('cat');
  });

  it('keeps completing command names after wrapper commands', () => {
    const context = getCommandCompletionContext('sudo ');
    const suggestions = getCommandSuggestions('sudo sy', 5).map((item) => item.text);

    expect(context.isCommandPosition).toBe(true);
    expect(suggestions).toContain('systemctl');
  });

  it('resets command context after shell separators', () => {
    const context = getCommandCompletionContext('cd /srv && gi');
    const suggestions = getCommandSuggestions('cd /srv && gi', 5).map((item) => item.text);

    expect(context.isCommandPosition).toBe(true);
    expect(context.currentToken).toBe('gi');
    expect(suggestions).toContain('git');
  });

  it('does not add weak contains matches for single-character command input', () => {
    const suggestions = getCommandSuggestions('w', 50).map((item) => item.text);

    expect(suggestions).toContain('wc');
    expect(suggestions).not.toContain('awk');
  });
});
