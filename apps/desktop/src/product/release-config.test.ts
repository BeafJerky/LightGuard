import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// Vitest runs this suite in the jsdom environment, so `import.meta.url` is an
// http:// URL rather than a file: one. Walk up from the working directory
// (apps/desktop under `pnpm --filter`) to the workspace root that owns the
// Tauri config instead.
const findRoot = (start: string): string => {
  let dir = resolve(start);
  for (;;) {
    if (existsSync(resolve(dir, 'apps/desktop/src-tauri/tauri.conf.json'))) {
      return dir;
    }
    const parent = dirname(dir);
    if (parent === dir) {
      throw new Error(`release config root not found above ${start}`);
    }
    dir = parent;
  }
};

const root = findRoot(process.cwd());
const read = (path: string) => readFileSync(`${root}/${path}`, 'utf8');
const readJson = (path: string) => JSON.parse(read(path)) as Record<string, any>;

describe('release configuration', () => {
  it('enables updater artifacts and uses four HTTPS endpoints', () => {
    const conf = readJson('apps/desktop/src-tauri/tauri.conf.json');
    expect(conf.bundle.createUpdaterArtifacts).toBe(true);

    const updater = conf.plugins.updater;
    expect(typeof updater.pubkey).toBe('string');
    expect(updater.pubkey.trim().length).toBeGreaterThan(40);
    expect(updater.pubkey).not.toMatch(/\.(pub|key)$/);
    expect(updater.endpoints).toHaveLength(4);
    for (const endpoint of updater.endpoints) {
      expect(endpoint).toMatch(/^https:\/\//);
    }
    expect(updater.dangerousInsecureTransportProtocol ?? false).toBe(false);
  });

  it('keeps all five version references on the same version', () => {
    const version = readJson('apps/desktop/src-tauri/tauri.conf.json').version as string;
    expect(readJson('apps/desktop/package.json').version).toBe(version);
    expect(read('apps/desktop/src-tauri/Cargo.toml')).toMatch(
      new RegExp(`^version = "${version}"$`, 'm'),
    );
    expect(read('apps/desktop/src-tauri/Cargo.lock')).toMatch(
      new RegExp(`name = "lightguard"\\r?\\nversion = "${version}"`),
    );
    expect(read('apps/desktop/src/product/data.ts')).toContain(`APP_VERSION = '${version}'`);
  });
});
