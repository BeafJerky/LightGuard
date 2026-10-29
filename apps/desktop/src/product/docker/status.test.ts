import { describe, expect, it } from 'vitest';
import { PERMISSION_HINT, permissionHint } from './status';

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
