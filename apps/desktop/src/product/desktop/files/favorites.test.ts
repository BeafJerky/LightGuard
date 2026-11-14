import { beforeEach, describe, expect, it } from 'vitest';
import {
  loadFavorites,
  loadRecents,
  pushRecent,
  removeFavorite,
  removeRecent,
  toggleFavorite,
} from './favorites';

describe('favorites and recents', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('toggles favorites per server and deduplicates', () => {
    expect(toggleFavorite('srv_1', '/data')).toBe(true);
    expect(toggleFavorite('srv_1', '/data')).toBe(false);
    expect(loadFavorites('srv_1')).toEqual([]);
    toggleFavorite('srv_1', '/a');
    toggleFavorite('srv_1', '/b');
    expect(loadFavorites('srv_1')).toEqual(['/b', '/a']);
    expect(loadFavorites('srv_2')).toEqual([]);
  });

  it('keeps recents newest first and removes duplicates', () => {
    pushRecent('srv_1', '/a');
    pushRecent('srv_1', '/b');
    pushRecent('srv_1', '/a');
    expect(loadRecents('srv_1')).toEqual(['/a', '/b']);
    expect(removeRecent('srv_1', '/a')).toEqual(['/b']);
  });

  it('removes a favorite explicitly', () => {
    toggleFavorite('srv_1', '/a');
    expect(removeFavorite('srv_1', '/a')).toEqual([]);
  });
});