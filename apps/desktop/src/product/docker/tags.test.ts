import { describe, expect, it } from 'vitest';
import { TAG_RENDER_LIMIT, displayRepo, filterTags, sortTags, tagView } from './tags';

describe('sortTags', () => {
  it('keeps latest on top and lists versions newest first', () => {
    expect(
      sortTags(['latest', 'unstable', 'v1.2.0', 'v1.2.1', 'v2.0.0', 'v2.0.1']),
    ).toEqual(['latest', 'unstable', 'v2.0.1', 'v2.0.0', 'v1.2.1', 'v1.2.0']);
  });

  it('compares version numbers numerically instead of as strings', () => {
    expect(sortTags(['1', '1.9', '1.10', '1.24-alpine', '1.24.0', 'latest'])).toEqual([
      'latest',
      '1.24.0',
      '1.24-alpine',
      '1.10',
      '1.9',
      '1',
    ]);
  });

  it('pins latest ahead of other non-version aliases', () => {
    expect(sortTags(['alpine', 'latest', 'stable'])).toEqual(['latest', 'alpine', 'stable']);
  });
});

describe('docker tag list helpers', () => {
  it('hides the library prefix only for official images', () => {
    expect(displayRepo('library/nginx')).toBe('nginx');
    expect(displayRepo('bitnami/nginx')).toBe('bitnami/nginx');
    expect(displayRepo('nginx')).toBe('nginx');
  });

  it('filters tags case-insensitively and keeps the original order', () => {
    const tags = ['1.29', '1.29-alpine', 'latest', 'alpine'];
    expect(filterTags(tags, 'ALPINE')).toEqual(['1.29-alpine', 'alpine']);
    expect(filterTags(tags, '  ')).toBe(tags);
    expect(filterTags(tags, 'missing')).toEqual([]);
  });

  it('renders at most the limit and reports truncation', () => {
    const tags = Array.from({ length: TAG_RENDER_LIMIT + 12 }, (_, index) => `v${index}`);
    const view = tagView(tags, '');
    expect(view.shown).toHaveLength(TAG_RENDER_LIMIT);
    expect(view.matched).toHaveLength(TAG_RENDER_LIMIT + 12);
    expect(view.truncated).toBe(true);
    expect(view.total).toBe(TAG_RENDER_LIMIT + 12);

    const filtered = tagView(tags, 'v1');
    expect(filtered.truncated).toBe(false);
    expect(filtered.shown).toHaveLength(filtered.matched.length);
  });

  it('counts all tags while filtering a large list', () => {
    const view = tagView(['1', '2'], '2');
    expect(view.total).toBe(2);
    expect(view.shown).toEqual(['2']);
  });

  it('presents the newest tags first after filtering', () => {
    const tags = ['v1.0.0', 'v2.0.0', 'latest'];
    expect(tagView(tags, '').shown).toEqual(['latest', 'v2.0.0', 'v1.0.0']);
    expect(tagView(tags, 'v').shown).toEqual(['v2.0.0', 'v1.0.0']);
  });
});
