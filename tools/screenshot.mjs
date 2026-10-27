import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { launchBrowser } from './cdp.mjs';

const args = Object.fromEntries(process.argv.slice(2).map((arg) => {
  const [key, ...rest] = arg.replace(/^--/, '').split('=');
  return [key, rest.join('=') || 'true'];
}));

const theme = args.theme || 'dark';
const scope = args.scope || 'console';
const route = args.route || '#/overview';
const width = Number(args.width || 1440);
const height = Number(args.height || 900);
const out = resolve(args.out || join(tmpdir(), `lightguard-${theme}-${scope}.png`));

const browser = await launchBrowser({ width, height });
try {
  await browser.cdp.navigate(`${browser.origin}/index.html#/overview`);
  await browser.cdp.eval(`localStorage.setItem('lightguard.theme.${scope}', ${JSON.stringify(theme)})`);
  await browser.cdp.navigate(`${browser.origin}/index.html${route}`);
  await browser.cdp.eval(`document.fonts.ready.then(() => true)`);
  await new Promise((ok) => setTimeout(ok, 1400));
  await browser.cdp.screenshot(out);
  console.log(out);
} finally {
  await browser.close();
}
