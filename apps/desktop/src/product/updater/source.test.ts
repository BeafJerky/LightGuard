import { describe, expect, it } from 'vitest';
import { initialSourceDraft, normalizeUpdateSource, UPDATE_SOURCE_PRESETS } from './source';

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

describe('initialSourceDraft', () => {
  it('maps auto to the preset selection without a draft', () => {
    expect(initialSourceDraft('auto')).toEqual({ customMode: false, draft: '' });
  });

  it('maps every preset id to the preset selection without a draft', () => {
    for (const preset of UPDATE_SOURCE_PRESETS) {
      expect(initialSourceDraft(preset.id)).toEqual({ customMode: false, draft: '' });
    }
  });

  it('keeps a custom https url in the draft', () => {
    expect(initialSourceDraft('https://mirror.example.com/latest.json')).toEqual({
      customMode: true,
      draft: 'https://mirror.example.com/latest.json',
    });
  });

  // 先归一化再判预设：非法值与 http/带凭据的地址都退回 auto，因此按预设处理、草稿清空。
  it('normalizes invalid values back to the auto preset', () => {
    expect(initialSourceDraft('http://mirror.example.com/latest.json')).toEqual({
      customMode: false,
      draft: '',
    });
    expect(initialSourceDraft('not a url')).toEqual({ customMode: false, draft: '' });
    expect(initialSourceDraft('')).toEqual({ customMode: false, draft: '' });
  });
});
