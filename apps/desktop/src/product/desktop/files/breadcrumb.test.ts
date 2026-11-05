import { describe, expect, it } from 'vitest';
import { pathCrumbs, visibleCrumbs } from './breadcrumb';

describe('path crumbs', () => {
  it('turns an absolute path into clickable segments', () => {
    expect(pathCrumbs('/etc/ssh')).toEqual([
      { label: '/', path: '/' },
      { label: 'etc', path: '/etc' },
      { label: 'ssh', path: '/etc/ssh' },
    ]);
  });

  it('keeps the root compact and ignores trailing slashes', () => {
    expect(pathCrumbs('/')).toEqual([{ label: '/', path: '/' }]);
    expect(pathCrumbs('/etc/ssh/')).toEqual(pathCrumbs('/etc/ssh'));
    expect(pathCrumbs('')).toEqual([{ label: '/', path: '/' }]);
  });

  it('supports drive-letter paths without inventing a leading slash', () => {
    expect(pathCrumbs('C:/Users/me')).toEqual([
      { label: 'C:', path: 'C:' },
      { label: 'Users', path: 'C:/Users' },
      { label: 'me', path: 'C:/Users/me' },
    ]);
  });

  it('shows every segment when the path is short', () => {
    const view = visibleCrumbs(pathCrumbs('/home/test'), 5);
    expect(view).toHaveLength(3);
    expect(view.every((item) => item.kind === 'crumb')).toBe(true);
  });

  it('collapses the middle of a long path and keeps the tail visible', () => {
    const view = visibleCrumbs(pathCrumbs('/home/alice/projects/lightguard/src/app'), 5);
    expect(view).toHaveLength(5);
    expect(view[0]).toEqual({ kind: 'crumb', crumb: { label: '/', path: '/' } });
    expect(view[1].kind).toBe('ellipsis');
    if (view[1].kind === 'ellipsis') {
      expect(view[1].hidden).toEqual([
        { label: 'home', path: '/home' },
        { label: 'alice', path: '/home/alice' },
        { label: 'projects', path: '/home/alice/projects' },
      ]);
    }
    expect(view.slice(2)).toEqual([
      { kind: 'crumb', crumb: { label: 'lightguard', path: '/home/alice/projects/lightguard' } },
      { kind: 'crumb', crumb: { label: 'src', path: '/home/alice/projects/lightguard/src' } },
      { kind: 'crumb', crumb: { label: 'app', path: '/home/alice/projects/lightguard/src/app' } },
    ]);
  });
});
