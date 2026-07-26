import parseDiff from 'parse-diff';

export type DiffLineKind = 'context' | 'add' | 'delete';

export interface DiffLine {
  kind: DiffLineKind;
  oldLine?: number;
  newLine?: number;
  marker: ' ' | '+' | '-';
  content: string;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

export interface DiffFileModel {
  from?: string;
  to?: string;
  additions: number;
  deletions: number;
  hunks: DiffHunk[];
}

export interface LimitedDiffModel {
  files: DiffFileModel[];
  truncated: boolean;
}

export function buildDiffModel(input: string): DiffFileModel[] {
  return parseDiff(input).map((file) => ({
    from: file.from,
    to: file.to,
    additions: file.additions,
    deletions: file.deletions,
    hunks: file.chunks.map((chunk) => ({
      header: chunk.content,
      lines: chunk.changes.map((change): DiffLine => {
        if (change.type === 'add') {
          return {
            kind: 'add',
            newLine: change.ln,
            marker: '+',
            content: change.content.slice(1),
          };
        }
        if (change.type === 'del') {
          return {
            kind: 'delete',
            oldLine: change.ln,
            marker: '-',
            content: change.content.slice(1),
          };
        }
        return {
          kind: 'context',
          oldLine: change.ln1,
          newLine: change.ln2,
          marker: ' ',
          content: change.content.slice(1),
        };
      }),
    })),
  }));
}

export function summarizeDiff(files: DiffFileModel[]): { additions: number; deletions: number } {
  return files.reduce(
    (summary, file) => ({
      additions: summary.additions + file.additions,
      deletions: summary.deletions + file.deletions,
    }),
    { additions: 0, deletions: 0 }
  );
}

export function hasTextDiff(files: DiffFileModel[]): boolean {
  return files.some((file) => file.hunks.some((hunk) => hunk.lines.length > 0));
}

export function limitDiffLines(files: DiffFileModel[], maxLines: number): LimitedDiffModel {
  let remaining = Math.max(0, maxLines);
  let truncated = false;

  const limitedFiles = files.map((file) => ({
    ...file,
    hunks: file.hunks.flatMap((hunk) => {
      if (remaining === 0) {
        if (hunk.lines.length > 0) truncated = true;
        return [];
      }

      const lines = hunk.lines.slice(0, remaining);
      remaining -= lines.length;
      if (lines.length < hunk.lines.length) truncated = true;
      return [{ ...hunk, lines }];
    }),
  }));

  return { files: limitedFiles, truncated };
}
