export type FileViewerKind =
  | 'text'
  | 'image'
  | 'pdf'
  | 'video'
  | 'audio'
  | 'archive'
  | 'unsupported';

const TEXT_EXTENSIONS = new Set([
  'adoc', 'asc', 'asciidoc', 'astro', 'bash', 'bat', 'c', 'cc', 'cfg', 'clj', 'cljs',
  'cmd', 'conf', 'cpp', 'cs', 'css', 'csv', 'cxx', 'dart', 'diff', 'dockerignore', 'editorconfig',
  'env', 'erl', 'ex', 'exs', 'fish', 'fs', 'fsi', 'fsx', 'go', 'gitattributes', 'gitignore',
  'h', 'hpp', 'hrl', 'hs', 'htm', 'html', 'ini', 'java', 'js', 'json', 'json5', 'jsonc',
  'jsx', 'kt', 'kts', 'latex', 'less', 'log', 'lua', 'm', 'markdown', 'md', 'mdx', 'mjs',
  'mm', 'php', 'pl', 'pm', 'properties', 'ps1', 'py', 'pyw', 'r', 'rb', 'rst', 'rs',
  'sass', 'scala', 'scss', 'sh', 'sql', 'svelte', 'swift', 'tex', 'toml', 'ts', 'tsv',
  'tsx', 'txt', 'vue', 'xml', 'yaml', 'yml', 'zsh',
]);

const TEXT_FILENAMES = new Set([
  'authors', 'changelog', 'cmakelists.txt', 'contributing', 'dockerfile', 'gemfile', 'license',
  'makefile', 'readme',
]);

interface BrowserFileType {
  kind: 'image' | 'pdf' | 'video' | 'audio';
  mimeType: string;
}

const BROWSER_FILE_TYPES: Record<string, BrowserFileType> = {
  '3gp': { kind: 'video', mimeType: 'video/3gpp' },
  aac: { kind: 'audio', mimeType: 'audio/aac' },
  avif: { kind: 'image', mimeType: 'image/avif' },
  bmp: { kind: 'image', mimeType: 'image/bmp' },
  flac: { kind: 'audio', mimeType: 'audio/flac' },
  gif: { kind: 'image', mimeType: 'image/gif' },
  ico: { kind: 'image', mimeType: 'image/x-icon' },
  jpeg: { kind: 'image', mimeType: 'image/jpeg' },
  jpg: { kind: 'image', mimeType: 'image/jpeg' },
  m4a: { kind: 'audio', mimeType: 'audio/mp4' },
  m4v: { kind: 'video', mimeType: 'video/mp4' },
  mov: { kind: 'video', mimeType: 'video/quicktime' },
  mp3: { kind: 'audio', mimeType: 'audio/mpeg' },
  mp4: { kind: 'video', mimeType: 'video/mp4' },
  oga: { kind: 'audio', mimeType: 'audio/ogg' },
  ogg: { kind: 'audio', mimeType: 'audio/ogg' },
  ogv: { kind: 'video', mimeType: 'video/ogg' },
  opus: { kind: 'audio', mimeType: 'audio/ogg' },
  pdf: { kind: 'pdf', mimeType: 'application/pdf' },
  png: { kind: 'image', mimeType: 'image/png' },
  svg: { kind: 'image', mimeType: 'image/svg+xml' },
  wav: { kind: 'audio', mimeType: 'audio/wav' },
  webm: { kind: 'video', mimeType: 'video/webm' },
  webp: { kind: 'image', mimeType: 'image/webp' },
};
const ARCHIVE_EXTENSIONS = ['.tar.gz', '.tar.bz2', '.tar.xz', '.tgz', '.tbz2', '.txz', '.zip', '.tar', '.gz', '.bz2', '.xz', '.7z', '.rar'];
const LISTABLE_ARCHIVE_EXTENSIONS = ['.tar.gz', '.tgz', '.zip', '.tar'];

function extensionOf(filename: string): string {
  const name = filename.toLowerCase();
  const dotIndex = name.lastIndexOf('.');
  return dotIndex > -1 ? name.slice(dotIndex + 1) : '';
}

export function getFileViewerKind(filename: string): FileViewerKind {
  const lowerName = filename.toLowerCase();
  const extension = extensionOf(filename);

  const browserFileType = BROWSER_FILE_TYPES[extension];
  if (browserFileType) return browserFileType.kind;
  if (ARCHIVE_EXTENSIONS.some((suffix) => lowerName.endsWith(suffix))) return 'archive';
  if (
    TEXT_EXTENSIONS.has(extension)
    || TEXT_FILENAMES.has(lowerName)
    || (lowerName.startsWith('.') && !lowerName.slice(1).includes('.'))
  ) {
    return 'text';
  }
  return 'unsupported';
}

export function getBrowserMimeType(filename: string, backendMimeType: string): string {
  if (backendMimeType && backendMimeType !== 'application/octet-stream') return backendMimeType;
  return BROWSER_FILE_TYPES[extensionOf(filename)]?.mimeType ?? 'application/octet-stream';
}

export function isArchiveListable(filename: string): boolean {
  const lowerName = filename.toLowerCase();
  return LISTABLE_ARCHIVE_EXTENSIONS.some((suffix) => lowerName.endsWith(suffix));
}

export function shouldReadAsBinary(kind: FileViewerKind): boolean {
  return kind !== 'text' && kind !== 'unsupported';
}

export const TEXT_PREVIEW_LIMIT_BYTES = 4 * 1024 * 1024;
export const BINARY_PREVIEW_LIMIT_BYTES = 64 * 1024 * 1024;
export const ARCHIVE_PREVIEW_LIMIT_BYTES = 32 * 1024 * 1024;
