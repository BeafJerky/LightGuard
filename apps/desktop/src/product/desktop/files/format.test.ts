import { describe, expect, it } from 'vitest';
import type { FileEntry } from '../../ssh/backend';
import {
  fileColorIcon,
  fileIcon,
  fileTone,
  fileTypeLabel,
  formatSize,
  formatTime,
  isImagePreviewable,
  isTextPreviewable,
} from './format';

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

  it('labels types', () => {
    expect(fileTypeLabel(entry({ kind: 'dir', name: 'logs' }))).toBe('文件夹');
    expect(fileTypeLabel(entry({ name: 'app.log' }))).toBe('LOG 文件');
  });

  it('classifies folders and links', () => {
    expect(fileIcon(entry({ kind: 'dir', name: 'logs' }))).toBe('folder');
    expect(fileColorIcon(entry({ kind: 'dir', name: 'logs' }))).toBe('folder');
    expect(fileIcon(entry({ name: 'docs', kind: 'symlink', targetKind: 'dir' }))).toBe('folder');
    expect(fileIcon(entry({ name: 'link', kind: 'symlink', targetKind: 'file' }))).toBe('link');
    expect(fileColorIcon(entry({ name: 'link', kind: 'symlink', targetKind: 'file' }))).toBeNull();
  });

  it('classifies code, config and shell files', () => {
    expect(fileIcon(entry({ name: 'main.rs' }))).toBe('file-code');
    expect(fileColorIcon(entry({ name: 'main.rs' }))).toBe('rust');
    expect(fileColorIcon(entry({ name: 'app.ts' }))).toBe('typescript');
    expect(fileColorIcon(entry({ name: 'App.vue' }))).toBe('vue');
    expect(fileColorIcon(entry({ name: 'index.js' }))).toBe('js');

    expect(fileIcon(entry({ name: 'nginx.conf' }))).toBe('file-config');
    expect(fileColorIcon(entry({ name: 'nginx.conf' }))).toBe('config');
    expect(fileColorIcon(entry({ name: 'helm.yaml' }))).toBe('yaml');
    expect(fileIcon(entry({ name: 'deploy.sh' }))).toBe('file-terminal');
    expect(fileColorIcon(entry({ name: 'deploy.sh' }))).toBe('shell');
  });

  it('classifies data, media and documents', () => {
    expect(fileIcon(entry({ name: 'package.json' }))).toBe('file-data');
    expect(fileColorIcon(entry({ name: 'package.json' }))).toBe('json');
    expect(fileIcon(entry({ name: 'db.sqlite' }))).toBe('file-data');
    expect(fileColorIcon(entry({ name: 'db.sqlite' }))).toBe('sqlite');
    expect(fileIcon(entry({ name: 'rows.csv' }))).toBe('file-spreadsheet');
    expect(fileColorIcon(entry({ name: 'rows.csv' }))).toBe('csv');

    expect(fileIcon(entry({ name: 'shot.PNG' }))).toBe('file-image');
    expect(fileColorIcon(entry({ name: 'shot.PNG' }))).toBe('image');
    expect(fileIcon(entry({ name: 'bundle.tar.gz' }))).toBe('file-archive');
    expect(fileColorIcon(entry({ name: 'bundle.tar.gz' }))).toBe('zip');
    expect(fileIcon(entry({ name: 'clip.mp4' }))).toBe('file-video');
    expect(fileColorIcon(entry({ name: 'clip.mp4' }))).toBe('video');
    expect(fileIcon(entry({ name: 'track.flac' }))).toBe('file-audio');
    expect(fileIcon(entry({ name: 'Inter.woff2' }))).toBe('file-font');
    expect(fileIcon(entry({ name: 'manual.pdf' }))).toBe('file-doc');
    expect(fileColorIcon(entry({ name: 'manual.pdf' }))).toBe('pdf');
    expect(fileIcon(entry({ name: 'id_rsa.pem' }))).toBe('file-key');
    expect(fileColorIcon(entry({ name: 'id_rsa.pem' }))).toBe('key');
  });

  it('recognizes config and key files without extensions', () => {
    expect(fileIcon(entry({ name: 'ssh_config', mode: 0o644 }))).toBe('file-config');
    expect(fileTone(entry({ name: 'sshd_config' }))).toBe('warning');
    expect(fileIcon(entry({ name: 'id_ed25519', mode: 0o600 }))).toBe('file-key');
    expect(fileColorIcon(entry({ name: 'ssh_host_rsa_key' }))).toBe('key');
  });

  it('uses executable bits to separate binaries from plain files', () => {
    expect(fileIcon(entry({ name: 'nginx', mode: 0o755 }))).toBe('file-binary');
    expect(fileColorIcon(entry({ name: 'nginx', mode: 0o755 }))).toBe('binary');
    expect(fileIcon(entry({ name: 'notes', mode: 0o644 }))).toBe('file-text');
    expect(fileColorIcon(entry({ name: 'notes', mode: 0o644 }))).toBe('text');
    expect(fileIcon(entry({ name: 'data.bin' }))).toBe('file-binary');
    expect(fileIcon(entry({ name: 'weird.xyz' }))).toBe('file');
    expect(fileColorIcon(entry({ name: 'weird.xyz' }))).toBe('file');
  });

  it('assigns a category tone', () => {
    expect(fileTone(entry({ kind: 'dir', name: 'logs' }))).toBe('primary');
    expect(fileTone(entry({ name: 'link', kind: 'symlink' }))).toBe('accent');
    expect(fileTone(entry({ name: 'main.rs' }))).toBe('info');
    expect(fileTone(entry({ name: 'nginx.conf' }))).toBe('warning');
    expect(fileTone(entry({ name: 'package.json' }))).toBe('success');
    expect(fileTone(entry({ name: 'shot.png' }))).toBe('accent');
    expect(fileTone(entry({ name: 'clip.mp4' }))).toBe('danger');
    expect(fileTone(entry({ name: 'id_rsa.pem' }))).toBe('danger');
    expect(fileTone(entry({ name: 'nginx', mode: 0o755 }))).toBe('muted');
    expect(fileTone(entry({ name: 'notes' }))).toBe('neutral');
  });

  it('previews svg as text, never as an image', () => {
    expect(isTextPreviewable('icon.svg')).toBe(true);
    expect(isImagePreviewable('icon.svg')).toBe(false);
    expect(fileIcon(entry({ name: 'icon.svg' }))).toBe('file-image');
    expect(isImagePreviewable('shot.png')).toBe(true);
    expect(isTextPreviewable('main.rs')).toBe(true);
    expect(isTextPreviewable('nginx.conf')).toBe(true);
  });
});
