import { describe, expect, it } from 'vitest';
import type { FileEntry } from '../../ssh/backend';
import { fileIcon, fileTypeLabel, formatSize, formatTime } from './format';

function entry(overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name: 'a.txt',
    path: '/tmp/a.txt',
    kind: 'file',
    size: 2048,
    mtime: Date.UTC(2026, 9, 28, 8, 5),
    mode: 0o644,
    modeText: '-rw-r--r--',
    uid: 1000,
    gid: 1000,
    ...overrides,
  };
}

describe('file formatting', () => {
  it('formats sizes', () => {
    expect(formatSize(0)).toBe('0 B');
    expect(formatSize(1023)).toBe('1023 B');
    expect(formatSize(2048)).toBe('2.0 KB');
    expect(formatSize(5 * 1024 * 1024)).toBe('5.0 MB');
  });

  it('formats local time', () => {
    expect(formatTime(null)).toBe('—');
    expect(formatTime(entry().mtime)).toMatch(/^2026-10-28 /);
  });

  it('labels types and icons', () => {
    expect(fileTypeLabel(entry({ kind: 'dir', name: 'logs' }))).toBe('文件夹');
    expect(fileTypeLabel(entry({ name: 'app.log' }))).toBe('LOG 文件');
    expect(fileIcon(entry({ kind: 'dir', name: 'logs' }))).toBe('folder');
    expect(fileIcon(entry({ name: 'shot.PNG' }))).toBe('image');
    expect(fileIcon(entry({ name: 'main.rs' }))).toBe('file-text');
    expect(fileIcon(entry({ name: 'data.bin' }))).toBe('file');
    expect(fileIcon(entry({ name: 'link', kind: 'symlink' }))).toBe('link');
  });
});
