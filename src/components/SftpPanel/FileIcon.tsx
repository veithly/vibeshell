import {
  Folder,
  FolderOpen,
  File,
  FileText,
  FileCode,
  FileJson,
  FileImage,
  FileVideo,
  FileAudio,
  FileArchive,
  FileSpreadsheet,
  Database,
  Settings,
  Lock,
  Key,
  Terminal,
  Binary,
  Globe,
  Package,
  BookOpen,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '../../lib/utils';

/**
 * File type categories for icon mapping
 */
type FileCategory =
  | 'folder'
  | 'text'
  | 'code'
  | 'data'
  | 'image'
  | 'video'
  | 'audio'
  | 'archive'
  | 'spreadsheet'
  | 'document'
  | 'config'
  | 'lock'
  | 'key'
  | 'shell'
  | 'binary'
  | 'web'
  | 'package'
  | 'database'
  | 'unknown';

/**
 * Comprehensive extension to file category mapping
 */
const EXTENSION_MAP: Record<string, FileCategory> = {
  // Text files
  txt: 'text',
  log: 'text',
  readme: 'text',
  changelog: 'text',
  license: 'text',
  authors: 'text',

  // Code files
  js: 'code',
  jsx: 'code',
  ts: 'code',
  tsx: 'code',
  py: 'code',
  pyw: 'code',
  rb: 'code',
  go: 'code',
  rs: 'code',
  c: 'code',
  h: 'code',
  cpp: 'code',
  hpp: 'code',
  cc: 'code',
  cxx: 'code',
  cs: 'code',
  java: 'code',
  kt: 'code',
  kts: 'code',
  scala: 'code',
  clj: 'code',
  cljs: 'code',
  php: 'code',
  swift: 'code',
  m: 'code',
  mm: 'code',
  pl: 'code',
  pm: 'code',
  r: 'code',
  lua: 'code',
  dart: 'code',
  ex: 'code',
  exs: 'code',
  erl: 'code',
  hrl: 'code',
  hs: 'code',
  lhs: 'code',
  ml: 'code',
  mli: 'code',
  fs: 'code',
  fsi: 'code',
  fsx: 'code',
  vue: 'code',
  svelte: 'code',
  astro: 'code',

  // Data/JSON files
  json: 'data',
  jsonc: 'data',
  json5: 'data',
  geojson: 'data',

  // Web files
  html: 'web',
  htm: 'web',
  xhtml: 'web',
  css: 'web',
  scss: 'web',
  sass: 'web',
  less: 'web',
  styl: 'web',

  // Markup/Document files
  md: 'document',
  markdown: 'document',
  mdx: 'document',
  rst: 'document',
  adoc: 'document',
  asciidoc: 'document',
  tex: 'document',
  latex: 'document',
  pdf: 'document',
  doc: 'document',
  docx: 'document',
  odt: 'document',
  rtf: 'document',

  // Image files
  png: 'image',
  jpg: 'image',
  jpeg: 'image',
  gif: 'image',
  bmp: 'image',
  ico: 'image',
  svg: 'image',
  webp: 'image',
  avif: 'image',
  tiff: 'image',
  tif: 'image',
  psd: 'image',
  ai: 'image',
  eps: 'image',
  raw: 'image',
  cr2: 'image',
  nef: 'image',
  heic: 'image',
  heif: 'image',

  // Video files
  mp4: 'video',
  webm: 'video',
  mkv: 'video',
  avi: 'video',
  mov: 'video',
  wmv: 'video',
  flv: 'video',
  m4v: 'video',
  '3gp': 'video',
  ogv: 'video',

  // Audio files
  mp3: 'audio',
  wav: 'audio',
  ogg: 'audio',
  flac: 'audio',
  aac: 'audio',
  wma: 'audio',
  m4a: 'audio',
  opus: 'audio',
  mid: 'audio',
  midi: 'audio',

  // Archive files
  zip: 'archive',
  tar: 'archive',
  gz: 'archive',
  tgz: 'archive',
  bz2: 'archive',
  xz: 'archive',
  '7z': 'archive',
  rar: 'archive',
  cab: 'archive',
  dmg: 'archive',
  iso: 'archive',

  // Spreadsheet files
  xls: 'spreadsheet',
  xlsx: 'spreadsheet',
  csv: 'spreadsheet',
  tsv: 'spreadsheet',
  ods: 'spreadsheet',

  // Config files
  yaml: 'config',
  yml: 'config',
  toml: 'config',
  ini: 'config',
  conf: 'config',
  cfg: 'config',
  env: 'config',
  properties: 'config',
  editorconfig: 'config',
  prettierrc: 'config',
  eslintrc: 'config',
  babelrc: 'config',
  gitignore: 'config',
  gitattributes: 'config',
  dockerignore: 'config',
  npmrc: 'config',
  nvmrc: 'config',

  // Lock files
  lock: 'lock',

  // Key files
  pem: 'key',
  key: 'key',
  pub: 'key',
  crt: 'key',
  cer: 'key',
  ca: 'key',
  p12: 'key',
  pfx: 'key',

  // Shell scripts
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  ksh: 'shell',
  csh: 'shell',
  tcsh: 'shell',
  ps1: 'shell',
  psm1: 'shell',
  bat: 'shell',
  cmd: 'shell',

  // Binary/Executable
  exe: 'binary',
  dll: 'binary',
  so: 'binary',
  dylib: 'binary',
  a: 'binary',
  o: 'binary',
  obj: 'binary',
  bin: 'binary',
  out: 'binary',
  app: 'binary',

  // Package files
  deb: 'package',
  rpm: 'package',
  apk: 'package',
  msi: 'package',
  pkg: 'package',

  // Database files
  db: 'database',
  sqlite: 'database',
  sqlite3: 'database',
  sql: 'database',
  mdb: 'database',
  accdb: 'database',
};

/**
 * Special filename mappings (exact matches)
 */
const FILENAME_MAP: Record<string, FileCategory> = {
  dockerfile: 'config',
  makefile: 'config',
  cmakelists: 'config',
  'package.json': 'data',
  'package-lock.json': 'lock',
  'yarn.lock': 'lock',
  'pnpm-lock.yaml': 'lock',
  'composer.lock': 'lock',
  'cargo.lock': 'lock',
  'gemfile.lock': 'lock',
  'poetry.lock': 'lock',
  '.env': 'config',
  '.env.local': 'config',
  '.env.development': 'config',
  '.env.production': 'config',
  '.gitignore': 'config',
  '.gitattributes': 'config',
  '.dockerignore': 'config',
  '.editorconfig': 'config',
  '.prettierrc': 'config',
  '.eslintrc': 'config',
  'tsconfig.json': 'data',
  'vite.config.ts': 'config',
  'vite.config.js': 'config',
  'webpack.config.js': 'config',
  'rollup.config.js': 'config',
  'tailwind.config.js': 'config',
  'tailwind.config.ts': 'config',
  readme: 'document',
  license: 'document',
  changelog: 'document',
  contributing: 'document',
};

/**
 * Icon and color mapping for each category
 */
const CATEGORY_CONFIG: Record<FileCategory, { icon: LucideIcon; colorClass: string }> = {
  folder: { icon: Folder, colorClass: 'text-tokyo-yellow' },
  text: { icon: FileText, colorClass: 'text-tokyo-fg' },
  code: { icon: FileCode, colorClass: 'text-tokyo-green' },
  data: { icon: FileJson, colorClass: 'text-tokyo-yellow' },
  image: { icon: FileImage, colorClass: 'text-tokyo-magenta' },
  video: { icon: FileVideo, colorClass: 'text-tokyo-red' },
  audio: { icon: FileAudio, colorClass: 'text-tokyo-purple' },
  archive: { icon: FileArchive, colorClass: 'text-tokyo-orange' },
  spreadsheet: { icon: FileSpreadsheet, colorClass: 'text-tokyo-green' },
  document: { icon: BookOpen, colorClass: 'text-tokyo-blue' },
  config: { icon: Settings, colorClass: 'text-tokyo-comment' },
  lock: { icon: Lock, colorClass: 'text-tokyo-red' },
  key: { icon: Key, colorClass: 'text-tokyo-yellow' },
  shell: { icon: Terminal, colorClass: 'text-tokyo-cyan' },
  binary: { icon: Binary, colorClass: 'text-tokyo-comment' },
  web: { icon: Globe, colorClass: 'text-tokyo-cyan' },
  package: { icon: Package, colorClass: 'text-tokyo-purple' },
  database: { icon: Database, colorClass: 'text-tokyo-blue' },
  unknown: { icon: File, colorClass: 'text-tokyo-fg' },
};

/**
 * Get file category from filename
 */
export function getFileCategory(filename: string, isDirectory: boolean): FileCategory {
  if (isDirectory) {
    return 'folder';
  }

  const lowerName = filename.toLowerCase();

  // Check exact filename matches first
  if (FILENAME_MAP[lowerName]) {
    return FILENAME_MAP[lowerName];
  }

  // Check for hidden config files
  if (lowerName.startsWith('.') && !lowerName.includes('.', 1)) {
    return 'config';
  }

  // Get extension
  const lastDot = filename.lastIndexOf('.');
  if (lastDot === -1 || lastDot === filename.length - 1) {
    return 'unknown';
  }

  const extension = filename.substring(lastDot + 1).toLowerCase();
  return EXTENSION_MAP[extension] || 'unknown';
}

/**
 * Check if file is previewable as text
 */
export function isTextPreviewable(filename: string): boolean {
  const category = getFileCategory(filename, false);
  return ['text', 'code', 'data', 'config', 'shell', 'web', 'document'].includes(category);
}

/**
 * Check if file is previewable as image
 */
export function isImagePreviewable(filename: string): boolean {
  const category = getFileCategory(filename, false);
  if (category !== 'image') return false;

  // Only preview web-compatible image formats
  const ext = filename.substring(filename.lastIndexOf('.') + 1).toLowerCase();
  return ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'bmp', 'ico', 'avif'].includes(ext);
}

/**
 * Get syntax highlighting language for a file
 */
export function getSyntaxLanguage(filename: string): string {
  const ext = filename.substring(filename.lastIndexOf('.') + 1).toLowerCase();

  const languageMap: Record<string, string> = {
    // JavaScript/TypeScript
    js: 'javascript',
    jsx: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    mjs: 'javascript',
    cjs: 'javascript',

    // Python
    py: 'python',
    pyw: 'python',

    // Web
    html: 'html',
    htm: 'html',
    css: 'css',
    scss: 'scss',
    sass: 'sass',
    less: 'less',

    // Data
    json: 'json',
    jsonc: 'json',
    json5: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    toml: 'toml',
    xml: 'xml',

    // Shell
    sh: 'bash',
    bash: 'bash',
    zsh: 'bash',
    fish: 'fish',
    ps1: 'powershell',
    bat: 'batch',
    cmd: 'batch',

    // Systems
    c: 'c',
    h: 'c',
    cpp: 'cpp',
    hpp: 'cpp',
    cc: 'cpp',
    cxx: 'cpp',
    rs: 'rust',
    go: 'go',
    java: 'java',
    kt: 'kotlin',
    swift: 'swift',

    // Markup
    md: 'markdown',
    markdown: 'markdown',
    rst: 'rst',
    tex: 'latex',

    // Database
    sql: 'sql',

    // Config
    ini: 'ini',
    conf: 'ini',
    cfg: 'ini',
    env: 'shell',
    dockerfile: 'dockerfile',

    // Ruby
    rb: 'ruby',

    // PHP
    php: 'php',

    // Lua
    lua: 'lua',
  };

  const lowerName = filename.toLowerCase();
  if (lowerName === 'dockerfile') return 'dockerfile';
  if (lowerName === 'makefile') return 'makefile';

  return languageMap[ext] || 'text';
}

interface FileIconProps {
  /** File or folder name */
  filename: string;
  /** Whether this is a directory */
  isDirectory: boolean;
  /** Whether the folder is open (only applies to directories) */
  isOpen?: boolean;
  /** Icon size class */
  size?: 'sm' | 'md' | 'lg';
  /** Additional CSS classes */
  className?: string;
}

/**
 * File icon component that displays appropriate icon based on file type
 */
export function FileIcon({
  filename,
  isDirectory,
  isOpen = false,
  size = 'md',
  className,
}: FileIconProps) {
  const category = getFileCategory(filename, isDirectory);
  const config = CATEGORY_CONFIG[category];

  // Use FolderOpen for open directories
  const IconComponent = isDirectory && isOpen ? FolderOpen : config.icon;

  const sizeClasses = {
    sm: 'w-3 h-3',
    md: 'w-4 h-4',
    lg: 'w-5 h-5',
  };

  return (
    <IconComponent
      className={cn(
        sizeClasses[size],
        config.colorClass,
        'flex-shrink-0',
        className
      )}
    />
  );
}

export type { FileIconProps };
