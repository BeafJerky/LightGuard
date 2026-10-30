import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/**
 * `LgIcon` 只按名称引用雪碧图里的 symbol；名称拼错或 symbol 缺失时组件会渲染成空白，
 * 视觉上表现为「按钮里少一个图标」。这里静态校验源码中出现的图标名都已注册。
 * 动态名（如 :name="option.icon"）无法静态解析，不在此校验范围。
 */

const pkgRoot = fileURLToPath(new URL('../..', import.meta.url));
const repoRoot = join(pkgRoot, '..', '..');
const spriteSource = readFileSync(join(pkgRoot, 'src', 'components', 'LgIconSprite.vue'), 'utf8');
const symbolIds = [...spriteSource.matchAll(/<symbol id="lg-i-([a-z0-9-]+)"/g)].map((m) => m[1]);

const scanRoots = [join(pkgRoot, 'src'), join(repoRoot, 'apps', 'desktop', 'src')];

function collectSources(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist') continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...collectSources(path));
    else if (/\.(vue|ts)$/.test(entry.name) && !/\.test\.ts$/.test(entry.name)) files.push(path);
  }
  return files;
}

const staticIconNames = (() => {
  const names = new Map<string, string[]>();
  for (const root of scanRoots) {
    for (const file of collectSources(root)) {
      const source = readFileSync(file, 'utf8');
      for (const match of source.matchAll(/<LgIcon[^>]*?\sname="([a-z0-9-]+)"/g)) {
        const list = names.get(match[1]) ?? [];
        if (!list.includes(file)) list.push(file);
        names.set(match[1], list);
      }
    }
  }
  return names;
})();

describe('LgIcon sprite', () => {
  it('未重复注册同名 symbol', () => {
    expect(new Set(symbolIds).size).toBe(symbolIds.length);
  });

  it('源码里静态引用的图标名都已注册', () => {
    const missing = [...staticIconNames.keys()].filter((name) => !symbolIds.includes(name));
    expect(missing).toEqual([]);
  });

  it('图标名使用小写短横线命名', () => {
    const invalid = [...staticIconNames.keys()].filter((name) => !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(name));
    expect(invalid).toEqual([]);
  });
});
