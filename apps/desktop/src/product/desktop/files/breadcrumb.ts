export interface PathCrumb {
  label: string;
  path: string;
}

export type CrumbView =
  | { kind: 'crumb'; crumb: PathCrumb }
  | { kind: 'ellipsis'; hidden: PathCrumb[] };

export function pathCrumbs(path: string): PathCrumb[] {
  const trimmed = path.replace(/\/+$/, '');
  if (!trimmed) return [{ label: '/', path: '/' }];
  const absolute = trimmed.startsWith('/');
  const parts = trimmed.split('/').filter(Boolean);
  let prefix = absolute ? '/' : '';
  const crumbs: PathCrumb[] = absolute ? [{ label: '/', path: '/' }] : [];
  for (const part of parts) {
    prefix = prefix === '/' ? `/${part}` : prefix ? `${prefix}/${part}` : part;
    crumbs.push({ label: part, path: prefix });
  }
  return crumbs.length ? crumbs : [{ label: '/', path: '/' }];
}

export function visibleCrumbs(crumbs: PathCrumb[], max = 5): CrumbView[] {
  if (crumbs.length <= max) {
    return crumbs.map((crumb) => ({ kind: 'crumb' as const, crumb }));
  }
  const tail = Math.max(1, max - 2);
  const head = crumbs[0];
  const hidden = crumbs.slice(1, crumbs.length - tail);
  return [
    { kind: 'crumb', crumb: head },
    { kind: 'ellipsis', hidden },
    ...crumbs.slice(crumbs.length - tail).map((crumb) => ({ kind: 'crumb' as const, crumb })),
  ];
}
