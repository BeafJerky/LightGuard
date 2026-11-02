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
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const readJson = (path: string) => JSON.parse(read(path)) as Record<string, any>;

const readUpdater = (): Record<string, any> => {
  const conf = readJson('apps/desktop/src-tauri/tauri.conf.json');
  const plugins = conf.plugins;
  if (!plugins || typeof plugins !== 'object') {
    throw new Error('tauri.conf.json is missing a plugins object');
  }
  const updater = plugins.updater;
  if (!updater || typeof updater !== 'object') {
    throw new Error('tauri.conf.json is missing plugins.updater');
  }
  return updater as Record<string, any>;
};

describe('release configuration', () => {
  it('enables updater artifacts and uses four HTTPS endpoints', () => {
    const conf = readJson('apps/desktop/src-tauri/tauri.conf.json');
    expect(conf.bundle.createUpdaterArtifacts).toBe(true);

    const updater = readUpdater();
    const pubkey = typeof updater.pubkey === 'string' ? updater.pubkey.trim() : '';
    expect(pubkey.length).toBeGreaterThan(40);
    expect(pubkey).not.toMatch(/\.(pub|key)$/);
    expect(updater.endpoints).toHaveLength(4);
    for (const endpoint of updater.endpoints) {
      expect(endpoint).toMatch(/^https:\/\//);
    }
    expect(updater.dangerousInsecureTransportProtocol ?? false).toBe(false);

    // 顺序即回退顺序：先直连原始清单，再依次经三个镜像取国内清单。
    const endpoints = updater.endpoints as string[];
    expect(endpoints[0]).toContain('raw.githubusercontent.com/BeafJerky/LightGuard/');
    expect(endpoints[0]).toContain('/update/latest.json');
    expect(endpoints[0]).not.toContain('latest.cn.json');
    const proxies = ['gh-proxy.com', 'ghproxy.net', 'cdn.jsdelivr.net'];
    expect(endpoints.slice(1)).toHaveLength(proxies.length);
    proxies.forEach((proxy, index) => {
      expect(endpoints[index + 1]).toContain(proxy);
      expect(endpoints[index + 1]).toContain('latest.cn.json');
    });
  });

  it('keeps all five version references on the same version', () => {
    const version = readJson('apps/desktop/src-tauri/tauri.conf.json').version as string;
    expect(readJson('apps/desktop/package.json').version).toBe(version);

    const cargo = read('apps/desktop/src-tauri/Cargo.toml');
    const packageSection = cargo.match(/^\[package\]([\s\S]*?)(?=^\[|(?![\s\S]))/m)?.[1];
    if (!packageSection) throw new Error('Cargo.toml is missing a [package] section');
    const escaped = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    expect(packageSection).toMatch(new RegExp(`^version = "${escaped}"$`, 'm'));

    expect(read('apps/desktop/src-tauri/Cargo.lock')).toMatch(
      new RegExp(`name = "lightguard"\\r?\\nversion = "${escaped}"`),
    );
    expect(read('apps/desktop/src/product/data.ts')).toContain(`APP_VERSION = '${version}'`);
  });

  it('ships a strict content security policy', () => {
    // 复用文件顶部的 readJson：路径相对仓库根，不要传绝对路径。
    const conf = readJson('apps/desktop/src-tauri/tauri.conf.json');
    const security = conf.app?.security;
    if (!security) throw new Error('tauri.conf.json is missing app.security');

    const csp = typeof security.csp === 'string' ? security.csp : '';
    expect(csp.length).toBeGreaterThan(0);
    expect(csp).not.toContain('*');
    expect(csp).toContain("default-src 'self'");
    expect(csp).toContain("script-src 'self'");
    expect(csp).toContain("object-src 'none'");
    expect(csp).toContain("connect-src 'self' ipc: http://ipc.localhost");
    expect(csp).toContain("img-src 'self' data:");

    const devCsp = typeof security.devCsp === 'string' ? security.devCsp : '';
    expect(devCsp).toContain('http://127.0.0.1:5178');
    expect(devCsp).toContain('ws://127.0.0.1:5178');
  });
});
