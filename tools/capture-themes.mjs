import { mkdir, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { launchBrowser } from './cdp.mjs';

const OUT = resolve(import.meta.dirname, '..', '..', 'docs', 'screenshots');
await mkdir(OUT, { recursive: true });

const browser = await launchBrowser({ width: 1440, height: 900 });
const { cdp, origin } = browser;
const url = (hash) => `${origin}/index.html${hash}`;

async function settleFonts() {
  await cdp.eval('document.fonts.ready.then(() => true)');
}

async function settleFrames() {
  await cdp.eval(`new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))))`);
}

async function shot({
  file,
  hash,
  width = 1440,
  height = 900,
  consoleId = 'dark',
  desktopId = 'dark',
  scroll = '',
  scrollBlock = 'center',
  openWindow = false,
  sheet = false,
}) {
  await cdp.setViewport(width, height);
  await cdp.navigate(url('#/overview'));
  await cdp.waitFor(`!!document.querySelector('.app')`);
  await cdp.eval(`
    localStorage.setItem('lightguard.theme.console', ${JSON.stringify(consoleId)});
    localStorage.setItem('lightguard.theme.desktop', ${JSON.stringify(desktopId)});
  `);
  await cdp.navigate(url(hash));
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);

  if (hash.startsWith('#/desktop/')) {
    await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
    // The first visit plays a 720ms boot splash and auto-opens system info.
    // Wait it out so the splash can never cover the window screenshot.
    await cdp.waitFor(`!document.querySelector('#desktopBoot')`, 10000);
    if (openWindow && !(await cdp.eval(`!!document.querySelector('.os-window[data-win="about"]')`))) {
      await cdp.eval(`document.querySelector('[data-action="desktop-icon"][data-app="about"]')?.click()`);
    }
    await cdp.waitFor(`!!document.querySelector('.os-window[data-win="about"]')`, 10000);
    await cdp.waitFor(`!document.querySelector('.os-window[data-win="about"]').classList.contains('minimized')`);
  }

  if (hash === '#/settings') {
    await cdp.waitFor(`!!document.querySelector('[data-theme-group="console"] input[data-theme-option]')`);
    await cdp.waitFor(`!!document.querySelector('[data-theme-group="desktop"] input[data-theme-option]')`);
  }

  await settleFonts();
  await settleFrames();

  if (scroll) {
    await cdp.eval(`(() => {
      const target = document.querySelector(${JSON.stringify(scroll)});
      if (!target) return false;
      target.scrollIntoView({ block: ${JSON.stringify(scrollBlock)}, inline: 'nearest' });
      return true;
    })()`);
    await settleFrames();
    await delay(120);
  }

  if (sheet) {
    await cdp.eval(`document.querySelector('[data-action="open-theme-menu"][data-theme-scope="console"]').click()`);
    await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
    await cdp.waitFor(`document.body.classList.contains('theme-sheet-open')`);
    await settleFonts();
    await settleFrames();
    await cdp.eval(`document.querySelector('#console-theme-menu').getAnimations({ subtree: true }).forEach((a) => { try { a.finish(); } catch {} })`);
  }

  // Let timers, font swaps and scroll anchoring land on the same frame for every run.
  await settleFrames();
  await delay(350);

  const diagnostics = await cdp.eval(`({
    theme: document.documentElement.dataset.theme,
    pack: document.documentElement.dataset.themePack,
    scope: document.documentElement.dataset.themeScope,
    scrollY: Math.round(window.scrollY),
    contentScrollTop: Math.round(document.querySelector('.content')?.scrollTop || 0),
    scrollWidth: document.documentElement.scrollWidth,
    innerWidth: window.innerWidth,
    boot: !!document.querySelector('#desktopBoot'),
    window: !!document.querySelector('.os-window[data-win="about"]'),
    windowVisible: (() => {
      const win = document.querySelector('.os-window[data-win="about"]');
      if (!win) return null;
      const rect = win.getBoundingClientRect();
      return { top: Math.round(rect.top), bottom: Math.round(rect.bottom), left: Math.round(rect.left), right: Math.round(rect.right) };
    })(),
    sheet: (() => {
      const panel = document.querySelector('#console-theme-menu');
      if (!panel) return null;
      const rect = panel.getBoundingClientRect();
      return { top: Math.round(rect.top), bottom: Math.round(rect.bottom), options: panel.querySelectorAll('[data-theme-menu-option]').length };
    })(),
  })`);

  await cdp.screenshot(`${OUT}/${file}`);

  const activeId = hash.startsWith('#/desktop/') ? desktopId : consoleId;
  const expectedPack = ['claude', 'pixel', 'nes', 'tech'].includes(activeId) ? activeId : 'default';
  const expectedTheme = ['dark', 'pixel', 'tech'].includes(activeId) ? 'dark' : 'light';
  const expectedScope = hash.startsWith('#/desktop/') ? 'desktop' : 'console';

  const { size } = await stat(`${OUT}/${file}`);
  if (size < 8000) throw new Error(`${file}: suspiciously small screenshot (${size} bytes)`);
  if (diagnostics.theme !== expectedTheme || diagnostics.pack !== expectedPack || diagnostics.scope !== expectedScope) {
    throw new Error(`${file}: expected ${expectedTheme}/${expectedPack}/${expectedScope}, got ${diagnostics.theme}/${diagnostics.pack}/${diagnostics.scope}`);
  }
  if (diagnostics.boot) throw new Error(`${file}: boot splash still visible`);
  if (openWindow && !diagnostics.window) throw new Error(`${file}: expected about window`);
  if (sheet && diagnostics.sheet?.options !== 7) throw new Error(`${file}: expected 7 sheet options, got ${diagnostics.sheet?.options}`);
  if (diagnostics.scrollWidth > diagnostics.innerWidth) {
    throw new Error(`${file}: horizontal overflow ${diagnostics.scrollWidth} > ${diagnostics.innerWidth}`);
  }
  console.log(file, JSON.stringify(diagnostics), `${size}B`);
}

const settings = '#/settings';
const consoleGallery = '[data-theme-group="console"]';
const packGallery = '[data-theme-group="console"] .theme-gallery-grid.packs';

await shot({
  file: '21-settings-two-themes.png',
  hash: settings,
  consoleId: 'dark',
  desktopId: 'light',
  // Frame the whole appearance card: the two 7-option groups stack in one
  // column, so centering the scope block would cut the console group header.
  scroll: '.settings-card.wide',
  scrollBlock: 'start',
});

await shot({
  file: '22-mobile-settings-two-themes.png',
  hash: settings,
  width: 390,
  height: 844,
  consoleId: 'dark',
  desktopId: 'light',
  scroll: consoleGallery,
  scrollBlock: 'start',
});

for (const [index, id, name] of [
  [23, 'claude', 'claude'],
  [24, 'pixel', 'pixel'],
  [25, 'nes', 'nes'],
  [26, 'tech', 'tech'],
]) {
  await shot({
    file: `${index}-console-${name}.png`,
    hash: '#/overview',
    consoleId: id,
    desktopId: id,
  });
}

for (const [index, id, name] of [
  [27, 'claude', 'claude'],
  [28, 'pixel', 'pixel'],
  [29, 'nes', 'nes'],
  [30, 'tech', 'tech'],
]) {
  await shot({
    file: `${index}-desktop-${name}.png`,
    hash: '#/desktop/prod-web-01',
    consoleId: 'dark',
    desktopId: id,
    openWindow: true,
  });
}

await shot({
  file: '31-settings-theme-gallery.png',
  hash: settings,
  consoleId: 'pixel',
  desktopId: 'tech',
  scroll: packGallery,
});

await shot({
  file: '32-mobile-theme-sheet.png',
  hash: '#/overview',
  width: 390,
  height: 844,
  consoleId: 'pixel',
  desktopId: 'pixel',
  sheet: true,
});

await shot({
  file: '33-mobile-desktop-pixel.png',
  hash: '#/desktop/prod-web-01',
  width: 390,
  height: 844,
  consoleId: 'dark',
  desktopId: 'pixel',
  openWindow: true,
});

await browser.close();
