import { mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { launchBrowser } from './cdp.mjs';

const DIST = resolve(import.meta.dirname, '..', 'apps', 'desktop', 'dist');
const OUT = resolve(import.meta.dirname, '..', '..', 'docs', 'screenshots', 'lightui');
const THEMES = ['dark', 'light', 'claude', 'pixel', 'nes', 'tech'];

await mkdir(OUT, { recursive: true });

const { cdp, origin, close } = await launchBrowser({ width: 1440, height: 1000, serveDir: DIST });
const results = [];

try {
  await cdp.navigate(`${origin}/`);
  await cdp.waitFor("!!document.querySelector('.lg-scope[data-lg-scope=\"console\"]')");
  await delay(400);

  for (const theme of THEMES) {
    const index = THEMES.indexOf(theme) + 1;
    await cdp.eval(`(() => {
      for (const scope of document.querySelectorAll('.lg-scope')) {
        scope.querySelectorAll('.theme-chip')[${index}].click();
      }
      return true;
    })()`);
    await delay(500);

    const state = await cdp.eval(`(() => {
      const out = {};
      for (const scope of document.querySelectorAll('.lg-scope')) {
        const key = scope.dataset.lgScope;
        out[key] = {
          theme: scope.dataset.lgTheme,
          pack: scope.dataset.lgPack,
          mode: scope.dataset.lgMode,
          portalChildren: scope.querySelector('.lg-portal').childElementCount,
        };
      }
      return out;
    })()`);

    for (const scope of ['console', 'desktop']) {
      const rect = await cdp.eval(`(() => {
        const el = document.querySelector('.lg-scope[data-lg-scope="${scope}"]');
        const r = el.getBoundingClientRect();
        return { x: Math.round(r.x + scrollX), y: Math.round(r.y + scrollY), width: Math.round(r.width), height: Math.round(r.height) };
      })()`);
      await cdp.screenshot(join(OUT, `${theme}-${scope}.png`), rect);
    }

    results.push({ theme, ...state });
    console.log(`captured ${theme}`);
  }

  const external = cdp.requests.filter((url) => !url.startsWith(origin) && !url.startsWith('data:'));
  const report = { results, pageErrors: cdp.pageErrors, externalRequests: external };
  console.log(JSON.stringify(report, null, 2));

  if (cdp.pageErrors.length) throw new Error(`page errors: ${cdp.pageErrors.join(' | ')}`);
  if (external.length) throw new Error(`external requests: ${external.join(', ')}`);
  for (const item of results) {
    for (const scope of ['console', 'desktop']) {
      const actual = item[scope];
      if (actual.theme !== item.theme) {
        throw new Error(`${item.theme}/${scope} resolved to ${actual.theme}`);
      }
    }
  }
} finally {
  await close();
}
