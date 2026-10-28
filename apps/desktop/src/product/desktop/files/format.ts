import type { FileEntry } from '../../ssh/backend';

const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'log', 'conf', 'cfg', 'ini', 'env', 'toml', 'yaml', 'yml',
  'json', 'xml', 'html', 'htm', 'css', 'scss', 'js', 'jsx', 'ts', 'tsx', 'vue',
  'sh', 'bash', 'zsh', 'py', 'rb', 'go', 'rs', 'java', 'c', 'h', 'cpp', 'hpp',
  'sql', 'service', 'socket', 'pem', 'crt', 'properties',
]);

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico']);

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

export function fileIcon(entry: FileEntry): string {
  const resolved = entry.targetKind ?? entry.kind;
  if (resolved === 'dir') return 'folder';
  if (entry.kind === 'symlink') return 'link';
  const extension = extensionOf(entry.name);
  if (IMAGE_EXTENSIONS.has(extension)) return 'image';
  if (TEXT_EXTENSIONS.has(extension) || entry.name.startsWith('.')) return 'file-text';
  return 'file';
}

export function isTextPreviewable(name: string): boolean {
  return TEXT_EXTENSIONS.has(extensionOf(name)) || name.startsWith('.') || !name.includes('.');
}

export function isImagePreviewable(name: string): boolean {
  return IMAGE_EXTENSIONS.has(extensionOf(name));
}
