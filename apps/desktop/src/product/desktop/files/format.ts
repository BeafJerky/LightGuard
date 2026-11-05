import type { FileEntry } from '../../ssh/backend';

export type FileTone =
  | 'primary'
  | 'info'
  | 'success'
  | 'warning'
  | 'danger'
  | 'accent'
  | 'neutral'
  | 'muted';

export type FileCategory =
  | 'folder'
  | 'link'
  | 'text'
  | 'config'
  | 'code'
  | 'data'
  | 'image'
  | 'archive'
  | 'video'
  | 'audio'
  | 'key'
  | 'spreadsheet'
  | 'font'
  | 'terminal'
  | 'binary'
  | 'doc'
  | 'file';

export type FileColorIcon =
  | 'folder'
  | 'text'
  | 'markdown'
  | 'log'
  | 'config'
  | 'yaml'
  | 'toml'
  | 'git'
  | 'js'
  | 'typescript'
  | 'vue'
  | 'python'
  | 'ruby'
  | 'go'
  | 'rust'
  | 'java'
  | 'c'
  | 'cpp'
  | 'csharp'
  | 'php'
  | 'html'
  | 'css'
  | 'json'
  | 'xml'
  | 'sqlite'
  | 'csv'
  | 'excel'
  | 'image'
  | 'zip'
  | 'video'
  | 'audio'
  | 'key'
  | 'font'
  | 'shell'
  | 'binary'
  | 'pdf'
  | 'word'
  | 'file';

// Preview support intentionally stays narrower than icon classification: SVG is
// text here even though it gets an image icon.
const PREVIEW_TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'log', 'conf', 'cfg', 'ini', 'env', 'toml', 'yaml', 'yml',
  'json', 'xml', 'html', 'htm', 'svg', 'css', 'scss', 'js', 'jsx', 'ts', 'tsx', 'vue',
  'sh', 'bash', 'zsh', 'py', 'rb', 'go', 'rs', 'java', 'c', 'h', 'cpp', 'hpp',
  'sql', 'service', 'socket', 'pem', 'crt', 'properties',
]);

const PREVIEW_IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico']);

const CODE_EXTENSIONS = new Set([
  'js', 'jsx', 'mjs', 'cjs', 'ts', 'tsx', 'mts', 'cts', 'vue', 'svelte', 'astro',
  'py', 'rb', 'go', 'rs', 'java', 'kt', 'kts', 'scala', 'groovy',
  'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hxx', 'cs', 'php',
  'html', 'htm', 'css', 'scss', 'sass', 'less',
  'swift', 'dart', 'lua', 'pl', 'pm', 'r', 'clj', 'cljs', 'ex', 'exs', 'erl', 'hrl',
]);

const SHELL_EXTENSIONS = new Set(['sh', 'bash', 'zsh', 'fish', 'ksh', 'csh', 'ps1', 'bat', 'cmd']);

const CONFIG_EXTENSIONS = new Set([
  'conf', 'cfg', 'ini', 'env', 'toml', 'yaml', 'yml',
  'properties', 'service', 'socket', 'desktop', 'rules', 'plist',
]);

const DATA_EXTENSIONS = new Set(['json', 'jsonc', 'json5', 'xml', 'sql', 'sqlite', 'sqlite3', 'db']);

const IMAGE_EXTENSIONS = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'svg', 'avif', 'tiff', 'tif', 'heic',
]);

const ARCHIVE_EXTENSIONS = new Set([
  'zip', 'tar', 'gz', 'tgz', 'xz', 'bz2', '7z', 'rar', 'zst', 'lz4', 'cab', 'jar', 'apk',
]);

const AUDIO_EXTENSIONS = new Set([
  'mp3', 'flac', 'wav', 'ogg', 'm4a', 'aac', 'opus', 'wma', 'mid', 'midi',
]);

const VIDEO_EXTENSIONS = new Set([
  'mp4', 'mkv', 'avi', 'mov', 'webm', 'm4v', 'wmv', 'flv', 'mpg', 'mpeg', '3gp',
]);

const KEY_EXTENSIONS = new Set(['pem', 'key', 'crt', 'cer', 'p12', 'pfx', 'pub', 'gpg', 'asc', 'csr']);

const SHEET_EXTENSIONS = new Set(['xls', 'xlsx', 'ods', 'numbers', 'csv', 'tsv']);

const FONT_EXTENSIONS = new Set(['ttf', 'otf', 'woff', 'woff2', 'eot']);

const DOC_EXTENSIONS = new Set(['pdf', 'doc', 'docx', 'odt', 'pages', 'rtf']);

const BINARY_EXTENSIONS = new Set([
  'bin', 'so', 'exe', 'dll', 'deb', 'rpm', 'appimage', 'o', 'a', 'ko', 'class', 'wasm', 'msi',
]);

const TEXT_EXTENSIONS = new Set(['txt', 'md', 'markdown', 'log', 'rst', 'adoc', 'org', 'tex']);

const CONFIG_NAMES = new Set([
  'dockerfile', 'makefile', 'cmakelists.txt', '.editorconfig', '.npmrc', '.yarnrc',
  '.prettierrc', '.eslintrc', '.gitignore', '.gitattributes', '.gitmodules',
]);

const SHELL_NAMES = new Set([
  '.bashrc', '.bash_profile', '.bash_aliases', '.zshrc', '.zprofile', '.profile',
]);

const CODE_COLORS: Record<string, FileColorIcon> = {
  js: 'js',
  jsx: 'js',
  mjs: 'js',
  cjs: 'js',
  ts: 'typescript',
  tsx: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  vue: 'vue',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  rs: 'rust',
  java: 'java',
  c: 'c',
  h: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  php: 'php',
  html: 'html',
  htm: 'html',
  css: 'css',
  scss: 'css',
  sass: 'css',
  less: 'css',
};

const DOC_COLORS: Record<string, FileColorIcon> = {
  pdf: 'pdf',
  doc: 'word',
  docx: 'word',
  odt: 'word',
  pages: 'word',
  rtf: 'word',
};

export function extensionOf(name: string): string {
  const index = name.lastIndexOf('.');
  return index > 0 ? name.slice(index + 1).toLowerCase() : '';
}

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = bytes / 1024;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[index]}`;
}

export function formatTime(ms: number | null): string {
  if (!ms) return '—';
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return '—';
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

export function fileTypeLabel(entry: FileEntry): string {
  if (entry.kind === 'dir') return '文件夹';
  if (entry.kind === 'symlink') return '符号链接';
  if (entry.kind === 'other') return '特殊文件';
  const extension = extensionOf(entry.name);
  return extension ? `${extension.toUpperCase()} 文件` : '文件';
}

function isExecutable(entry: FileEntry): boolean {
  return entry.kind === 'file' && (entry.mode & 0o111) !== 0;
}

export function fileCategory(entry: FileEntry): FileCategory {
  const resolved = entry.targetKind ?? entry.kind;
  if (resolved === 'dir') return 'folder';
  if (entry.kind === 'symlink') return 'link';
  if (entry.kind === 'other') return 'binary';

  const name = entry.name.toLowerCase();
  const extension = extensionOf(name);

  if (CONFIG_NAMES.has(name) || SHELL_NAMES.has(name)) {
    return SHELL_NAMES.has(name) ? 'terminal' : 'config';
  }
  if (name.startsWith('.git')) return 'config';
  if (name.startsWith('.env')) return 'config';
  if (name.endsWith('config')) return 'config';
  if (/^id_(rsa|dsa|ecdsa|ed25519)/.test(name) || name.endsWith('_key')) return 'key';
  if (!extension) return isExecutable(entry) ? 'binary' : 'text';

  if (SHELL_EXTENSIONS.has(extension)) return 'terminal';
  if (CONFIG_EXTENSIONS.has(extension)) return 'config';
  if (CODE_EXTENSIONS.has(extension)) return 'code';
  if (DATA_EXTENSIONS.has(extension)) return 'data';
  if (SHEET_EXTENSIONS.has(extension)) return 'spreadsheet';
  if (IMAGE_EXTENSIONS.has(extension)) return 'image';
  if (ARCHIVE_EXTENSIONS.has(extension)) return 'archive';
  if (AUDIO_EXTENSIONS.has(extension)) return 'audio';
  if (VIDEO_EXTENSIONS.has(extension)) return 'video';
  if (KEY_EXTENSIONS.has(extension)) return 'key';
  if (FONT_EXTENSIONS.has(extension)) return 'font';
  if (DOC_EXTENSIONS.has(extension)) return 'doc';
  if (BINARY_EXTENSIONS.has(extension)) return 'binary';
  if (TEXT_EXTENSIONS.has(extension)) return 'text';
  return isExecutable(entry) ? 'binary' : 'file';
}

const LINE_ICONS: Record<FileCategory, string> = {
  folder: 'folder',
  link: 'link',
  text: 'file-text',
  config: 'file-config',
  code: 'file-code',
  data: 'file-data',
  image: 'file-image',
  archive: 'file-archive',
  video: 'file-video',
  audio: 'file-audio',
  key: 'file-key',
  spreadsheet: 'file-spreadsheet',
  font: 'file-font',
  terminal: 'file-terminal',
  binary: 'file-binary',
  doc: 'file-doc',
  file: 'file',
};

export function fileIcon(entry: FileEntry): string {
  return LINE_ICONS[fileCategory(entry)];
}

const TONES: Record<FileCategory, FileTone> = {
  folder: 'primary',
  link: 'accent',
  text: 'neutral',
  config: 'warning',
  code: 'info',
  data: 'success',
  image: 'accent',
  archive: 'warning',
  video: 'danger',
  audio: 'accent',
  key: 'danger',
  spreadsheet: 'success',
  font: 'accent',
  terminal: 'info',
  binary: 'muted',
  doc: 'danger',
  file: 'neutral',
};

export function fileTone(entry: FileEntry): FileTone {
  return TONES[fileCategory(entry)];
}

export function fileColorIcon(entry: FileEntry): FileColorIcon | null {
  const category = fileCategory(entry);
  if (category === 'link') return null;
  if (category === 'folder') return 'folder';

  const name = entry.name.toLowerCase();
  const extension = extensionOf(name);

  switch (category) {
    case 'text':
      if (extension === 'md' || extension === 'markdown') return 'markdown';
      if (extension === 'log') return 'log';
      return 'text';
    case 'config':
      if (name.startsWith('.git')) return 'git';
      if (extension === 'yaml' || extension === 'yml') return 'yaml';
      if (extension === 'toml') return 'toml';
      return 'config';
    case 'code':
      return CODE_COLORS[extension] ?? 'file';
    case 'terminal':
      return 'shell';
    case 'data':
      if (extension === 'xml') return 'xml';
      if (extension === 'sql' || extension === 'sqlite' || extension === 'sqlite3' || extension === 'db') {
        return 'sqlite';
      }
      return 'json';
    case 'spreadsheet':
      return extension === 'xls' || extension === 'xlsx' || extension === 'ods' || extension === 'numbers'
        ? 'excel'
        : 'csv';
    case 'image':
      return 'image';
    case 'archive':
      return 'zip';
    case 'audio':
      return 'audio';
    case 'video':
      return 'video';
    case 'key':
      return 'key';
    case 'font':
      return 'font';
    case 'binary':
      return 'binary';
    case 'doc':
      return DOC_COLORS[extension] ?? 'word';
    default:
      return 'file';
  }
}

export function isTextPreviewable(name: string): boolean {
  return PREVIEW_TEXT_EXTENSIONS.has(extensionOf(name)) || name.startsWith('.') || !name.includes('.');
}

export function isImagePreviewable(name: string): boolean {
  return PREVIEW_IMAGE_EXTENSIONS.has(extensionOf(name));
}
