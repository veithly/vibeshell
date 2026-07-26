import { describe, expect, it } from 'vitest';
import { getBrowserMimeType, getFileViewerKind, isArchiveListable } from './fileWorkspace';

describe('getFileViewerKind', () => {
  it.each([
    ['main.tsx', 'text'],
    ['Dockerfile', 'text'],
    ['README.md', 'text'],
    ['manual.pdf', 'pdf'],
    ['cover.webp', 'image'],
    ['demo.mp4', 'video'],
    ['theme.ogg', 'audio'],
    ['score.mid', 'unsupported'],
    ['release.tar.gz', 'archive'],
    ['backup.zip', 'archive'],
    ['report.docx', 'unsupported'],
  ] as const)('classifies %s as %s', (filename, kind) => {
    expect(getFileViewerKind(filename)).toBe(kind);
  });

  it('only advertises archive listings for formats the viewer can inspect', () => {
    expect(isArchiveListable('release.tar.gz')).toBe(true);
    expect(isArchiveListable('backup.zip')).toBe(true);
    expect(isArchiveListable('legacy.rar')).toBe(false);
  });

  it('uses the backend MIME type first and a shared browser fallback otherwise', () => {
    expect(getBrowserMimeType('track.aac', 'application/octet-stream')).toBe('audio/aac');
    expect(getBrowserMimeType('track.aac', 'audio/custom')).toBe('audio/custom');
  });
});
