import { describe, expect, it } from 'vitest';
import { getSyntaxLanguage } from '../FileIcon';

describe('getSyntaxLanguage', () => {
  it.each([
    ['service.cs', 'csharp'],
    ['widget.dart', 'dart'],
    ['Main.scala', 'scala'],
    ['worker.ex', 'elixir'],
    ['view.vue', 'xml'],
    ['component.svelte', 'xml'],
  ])('maps %s to a highlighter language instead of plain text', (filename, language) => {
    expect(getSyntaxLanguage(filename)).toBe(language);
  });
});
