import { describe, expect, it } from 'vitest';
import { themes } from './settingsStore';

function luminance(hex: string): number {
  const channels = hex.slice(1).match(/.{2}/g)?.map((channel) => parseInt(channel, 16) / 255) ?? [];
  const [red, green, blue] = channels.map((channel) => (
    channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
  ));
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(foreground: string, background: string): number {
  const lighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

describe('theme contrast', () => {
  for (const theme of themes) {
    it(`${theme.displayName} keeps text and actions readable`, () => {
      expect(contrast(theme.colors.fg, theme.colors.bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(theme.colors.fg, theme.colors.bgDark)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(theme.colors.fgDark, theme.colors.bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(theme.colors.onAccent, theme.colors.accent)).toBeGreaterThanOrEqual(4.5);
    });
  }
});
