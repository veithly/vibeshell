import { describe, expect, it } from 'vitest';
import { buildDiffModel, hasTextDiff, limitDiffLines, summarizeDiff } from './diffModel';

describe('buildDiffModel', () => {
  it('maps unified diff lines and line numbers', () => {
    const model = buildDiffModel([
      'diff --git a/src/a.ts b/src/a.ts',
      'index 1111111..2222222 100644',
      '--- a/src/a.ts',
      '+++ b/src/a.ts',
      '@@ -1,2 +1,2 @@',
      ' const value = 1;',
      '-oldValue();',
      '+newValue();',
      '',
    ].join('\n'));

    expect(model).toHaveLength(1);
    expect(model[0].hunks[0].lines).toEqual([
      { kind: 'context', oldLine: 1, newLine: 1, marker: ' ', content: 'const value = 1;' },
      { kind: 'delete', oldLine: 2, marker: '-', content: 'oldValue();' },
      { kind: 'add', newLine: 2, marker: '+', content: 'newValue();' },
    ]);
    expect(summarizeDiff(model)).toEqual({ additions: 1, deletions: 1 });
    expect(hasTextDiff(model)).toBe(true);
  });

  it('recognizes a binary diff with no hunks as having no text diff', () => {
    const model = buildDiffModel([
      'diff --git a/image.png b/image.png',
      'index 1111111..2222222 100644',
      'Binary files a/image.png and b/image.png differ',
      '',
    ].join('\n'));

    expect(model).toHaveLength(1);
    expect(model[0].hunks).toEqual([]);
    expect(hasTextDiff(model)).toBe(false);
  });

  it('caps rendered lines while preserving file and hunk context', () => {
    const model = buildDiffModel([
      'diff --git a/src/a.ts b/src/a.ts',
      '--- a/src/a.ts',
      '+++ b/src/a.ts',
      '@@ -1,3 +1,3 @@',
      ' first',
      '-second',
      '+replacement',
      ' third',
      '',
    ].join('\n'));

    const limited = limitDiffLines(model, 2);

    expect(limited.truncated).toBe(true);
    expect(limited.files[0].hunks[0].header).toContain('@@ -1,3 +1,3 @@');
    expect(limited.files[0].hunks[0].lines).toHaveLength(2);
  });
});
