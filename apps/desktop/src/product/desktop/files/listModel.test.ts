import { describe, expect, it } from 'vitest';
import type { FileEntry } from '../../ssh/backend';
import { moveSelection, toggleSort, visibleEntries } from './listModel';

function entry(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/tmp/${name}`,
    kind: 'file',
    size: name.length,
    mtime: 0,
    mode: 0o644,
    modeText: '-rw-r--r--',
    uid: 0,
    gid: 0,
    ...overrides,
  };
}

const entries = [
  entry('zeta.txt', { size: 10, mtime: 30 }),
  entry('alpha.txt', { size: 30, mtime: 10 }),
  entry('logs', { kind: 'dir', targetKind: 'dir', size: 0, mtime: 20 }),
  entry('.hidden', { size: 1 }),
  entry('中.txt', { size: 20, mtime: 40 }),
];

describe('file list model', () => {
  it('keeps directories first in both directions', () => {
    const asc = visibleEntries(entries, { sortKey: 'size', sortDir: 'asc', query: '', showHidden: false });
    const desc = visibleEntries(entries, { sortKey: 'size', sortDir: 'desc', query: '', showHidden: false });
    expect(asc[0].name).toBe('logs');
    expect(desc[0].name).toBe('logs');
  });

  it('filters hidden files and query', () => {
    expect(visibleEntries(entries, { sortKey: 'name', sortDir: 'asc', query: '', showHidden: false })).toHaveLength(4);
    expect(visibleEntries(entries, { sortKey: 'name', sortDir: 'asc', query: 'ALP', showHidden: false }).map((item) => item.name)).toEqual(['alpha.txt']);
  });

  it('sorts names naturally', () => {
    const a = entry('file2.txt');
    const b = entry('file10.txt');
    const sorted = visibleEntries([b, a], { sortKey: 'name', sortDir: 'asc', query: '', showHidden: true });
    expect(sorted.map((item) => item.name)).toEqual(['file2.txt', 'file10.txt']);
  });

  it('toggles sort direction on the same key', () => {
    expect(toggleSort({ sortKey: 'name', sortDir: 'asc' }, 'name')).toEqual({ sortKey: 'name', sortDir: 'desc' });
    expect(toggleSort({ sortKey: 'name', sortDir: 'asc' }, 'size')).toEqual({ sortKey: 'size', sortDir: 'asc' });
  });

  it('moves selection with bounds', () => {
    const rows = visibleEntries(entries, { sortKey: 'name', sortDir: 'asc', query: '', showHidden: false });
    expect(moveSelection(rows, rows[0].path, 'down', 2)).toBe(rows[1].path);
    expect(moveSelection(rows, rows[0].path, 'up', 2)).toBe(rows[0].path);
    expect(moveSelection(rows, rows[rows.length - 1].path, 'end', 2)).toBe(rows[rows.length - 1].path);
  });

  it('moves by rows and stays inside a row when columns are set', () => {
    const rows = ['a', 'b', 'c', 'd', 'e', 'f', 'g'].map((name) => entry(name));
    expect(moveSelection(rows, rows[4].path, 'up', 6, 3)).toBe(rows[1].path);
    expect(moveSelection(rows, rows[4].path, 'down', 6, 3)).toBe(rows[6].path);
    expect(moveSelection(rows, rows[1].path, 'right', 6, 3)).toBe(rows[2].path);
    expect(moveSelection(rows, rows[2].path, 'right', 6, 3)).toBe(rows[2].path);
    expect(moveSelection(rows, rows[3].path, 'left', 6, 3)).toBe(rows[3].path);
    expect(moveSelection(rows, rows[4].path, 'left', 6, 3)).toBe(rows[3].path);
    expect(moveSelection(rows, rows[6].path, 'right', 6, 3)).toBe(rows[6].path);
  });
});
