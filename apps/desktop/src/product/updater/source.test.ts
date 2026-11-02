import { describe, expect, it } from 'vitest';
import { normalizeUpdateSource, UPDATE_SOURCE_PRESETS } from './source';

describe('normalizeUpdateSource', () => {
  it('defaults to auto for non-strings', () => {
    expect(normalizeUpdateSource(undefined)).toBe('auto');
    expect(normalizeUpdateSource(42)).toBe('auto');
  });

  it('keeps https urls without credentials', () => {
    expect(normalizeUpdateSource(' https://mirror.example.com/latest.json ')).toBe(
      'https://mirror.example.com/latest.json',
    );
  });

  it('rejects http, credentials and malformed urls', () => {
    expect(normalizeUpdateSource('http://mirror.example.com/latest.json')).toBe('auto');
    expect(normalizeUpdateSource('https://user:pass@mirror.example.com/latest.json')).toBe('auto');
    expect(normalizeUpdateSource('not a url')).toBe('auto');
  });

  it('keeps preset urls stable through normalization', () => {
    for (const preset of UPDATE_SOURCE_PRESETS) {
      expect(normalizeUpdateSource(preset.id)).toBe(preset.id);
    }
  });
});
