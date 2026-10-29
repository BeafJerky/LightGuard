import { describe, expect, it } from 'vitest';
import type { DockerImage } from './api';
import { QUICK_REGISTRY, filterImages, imageReference, pullReference, tagSummary } from './images';

function image(patch: Partial<DockerImage> = {}): DockerImage {
  return {
    id: `sha256:${'a'.repeat(64)}`,
    shortId: 'aaaaaaaaaaaa',
    repo: 'nginx',
    tags: ['1.27'],
    size: '192MB',
    createdAt: '2026-10-29 09:00:00 +0800 CST',
    containers: 0,
    dangling: false,
    ...patch,
  };
}

describe('docker images', () => {
  it('filters by repository, tag and id', () => {
    const rows = [
      image(),
      image({ id: `sha256:${'b'.repeat(64)}`, repo: 'postgres', tags: ['16'] }),
    ];
    expect(filterImages(rows, 'nginx')).toHaveLength(1);
    expect(filterImages(rows, '16')).toHaveLength(1);
    expect(filterImages(rows, '  ')).toHaveLength(2);
  });

  it('summarises tags with a remainder count', () => {
    expect(tagSummary([])).toBe('<none>');
    expect(tagSummary(['1.27'])).toBe('1.27');
    expect(tagSummary(['a', 'b', 'c', 'd'])).toBe('a、b、c +1');
  });

  it('builds a removable reference', () => {
    expect(imageReference(image({ tags: ['1.27', 'latest'] }))).toBe('nginx:1.27');
    expect(imageReference(image({ tags: [] }))).toBe(`sha256:${'a'.repeat(64)}`);
    expect(imageReference(image({ dangling: true, repo: '<none>', tags: [] }))).toBe(
      `sha256:${'a'.repeat(64)}`,
    );
  });

  it('normalises pull references', () => {
    expect(pullReference('nginx', '1.27')).toBe('nginx:1.27');
    expect(pullReference('  redis  ', '')).toBe('redis:latest');
    expect(pullReference('nginx@sha256:abc', 'latest')).toBe('nginx@sha256:abc');
  });

  it('ships a quick registry list', () => {
    expect(QUICK_REGISTRY.length).toBeGreaterThanOrEqual(8);
    expect(QUICK_REGISTRY.some((item) => item.name === 'nginx')).toBe(true);
    expect(QUICK_REGISTRY.every((item) => item.name && item.description)).toBe(true);
  });
});
