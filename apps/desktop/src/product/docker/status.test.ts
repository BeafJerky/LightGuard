import { describe, expect, it } from 'vitest';
import {
  PERMISSION_HINT,
  REGISTRY_HINT,
  TAG_HINT,
  permissionHint,
  registryHint,
  tagHint,
} from './status';

describe('docker tag hint', () => {
  it('explains that tags come from the mirror instead of Docker Hub', () => {
    expect(tagHint('REGISTRY_FAILED')).toBe(TAG_HINT);
    expect(tagHint('TIMEOUT')).toBe('');
    expect(TAG_HINT).toContain('不经过 Docker Hub');
    expect(TAG_HINT).toContain('加速源设置');
  });
});

describe('docker registry hint', () => {
  it('explains the Docker Hub timeout only for registry failures', () => {
    expect(registryHint('REGISTRY_FAILED')).toBe(REGISTRY_HINT);
    expect(registryHint('TIMEOUT')).toBe('');
    expect(registryHint('DOCKER_PERMISSION')).toBe('');
  });

  it('separates search from the registry mirror', () => {
    expect(REGISTRY_HINT).toContain('加速源只作用于拉取');
    expect(REGISTRY_HINT).toContain('直接拉取');
  });
});

describe('docker permission hint', () => {
  it('adds the group hint only for socket permission errors', () => {
    expect(permissionHint('DOCKER_PERMISSION')).toBe(PERMISSION_HINT);
    expect(permissionHint('DOCKER_UNAVAILABLE')).toBe('');
    expect(permissionHint('')).toBe('');
  });

  it('mentions the docker group and no automatic elevation', () => {
    expect(PERMISSION_HINT).toContain('docker 组');
    expect(PERMISSION_HINT).toContain('不会自动提权');
  });
});
