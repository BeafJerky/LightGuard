import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { statSync } from 'node:fs';
import { resolve } from 'node:path';
import { launchBrowser } from './cdp.mjs';

const PROTO = resolve(import.meta.dirname, '..', '..', 'docs', 'prototype');

let browser;
let cdp;
let origin;

before(async () => {
  browser = await launchBrowser({ width: 1440, height: 900 });
  cdp = browser.cdp;
  origin = browser.origin;
});

after(async () => {
  await browser.close();
});

const url = (hash = '#/overview') => `${origin}/index.html${hash}`;

async function open(hash = '#/overview') {
  await cdp.navigate(url(hash));
  await cdp.waitFor(`!!document.querySelector('.app')`);
}

async function reset(hash = '#/overview') {
  await open(hash);
  await cdp.eval(`localStorage.clear()`);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
}

async function seed(scope, id) {
  await open('#/overview');
  await cdp.eval(`localStorage.setItem('lightguard.theme.${scope}', ${JSON.stringify(id)})`);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
}

async function attrs() {
  return cdp.eval(`({
    theme: document.documentElement.dataset.theme,
    pack: document.documentElement.dataset.themePack,
    scope: document.documentElement.dataset.themeScope,
    color: document.querySelector('meta[name="theme-color"]').content,
  })`);
}

test('seven theme ids resolve to the correct appearance and pack', async () => {
  const cases = [
    ['dark', 'dark', 'default', '#070B12'],
    ['light', 'light', 'default', '#F3F6FA'],
    ['claude', 'light', 'claude', '#F5F1E8'],
    ['pixel', 'dark', 'pixel', '#10131C'],
    ['nes', 'light', 'nes', '#EDEAE3'],
    ['tech', 'dark', 'tech', '#050B14'],
  ];
  for (const [id, theme, pack, color] of cases) {
    await seed('console', id);
    assert.deepEqual(await attrs(), { theme, pack, scope: 'console', color });
  }

  assert.deepEqual(
    await cdp.eval(`window.LightGuardPrototype.themeIds`),
    ['dark', 'light', 'system', 'claude', 'pixel', 'nes', 'tech'],
  );

  await cdp.setMedia([{ name: 'prefers-color-scheme', value: 'dark' }]);
  await seed('console', 'system');
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'default', scope: 'console', color: '#070B12' });
  await cdp.setMedia([{ name: 'prefers-color-scheme', value: 'light' }]);
  await cdp.reload();
  await cdp.waitFor(`document.documentElement.dataset.theme === 'light'`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'default', scope: 'console', color: '#F3F6FA' });
  await cdp.setMedia([]);
});

test('prepaint applies console pack before app.js runs', async () => {
  await open();
  await cdp.eval(`localStorage.setItem('lightguard.theme.console', 'pixel')`);
  await cdp.blockUrls(['*app.js*']);
  await cdp.reload();
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'pixel'`);
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'pixel', scope: 'console', color: '#10131C' });
  assert.equal(await cdp.eval(`document.querySelector('#view').children.length === 0`), true);
  await cdp.blockUrls([]);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
});

test('prepaint reads the desktop key when opening a desktop route directly', async () => {
  await open();
  await cdp.eval(`localStorage.setItem('lightguard.theme.desktop', 'nes')`);
  await cdp.blockUrls(['*app.js*']);
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'nes'`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'nes', scope: 'desktop', color: '#EDEAE3' });
  await cdp.blockUrls([]);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
});

test('console and desktop choices persist independently', async () => {
  await reset('#/settings');
  await cdp.eval(`window.LightGuardPrototype.setTheme('console', 'claude')`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'claude'`);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
  assert.equal((await attrs()).pack, 'claude');

  await seed('desktop', 'nes');
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'nes', scope: 'desktop', color: '#EDEAE3' });

  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('#view .settings-grid')`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'claude', scope: 'console', color: '#F5F1E8' });
});

test('legacy single theme key seeds both scopes', async () => {
  await reset();
  await cdp.eval(`localStorage.setItem('lightguard.theme', 'light')`);
  await cdp.reload();
  await cdp.waitFor(`document.documentElement.dataset.theme === 'light'`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'default', scope: 'console', color: '#F3F6FA' });
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'default', scope: 'desktop', color: '#F3F6FA' });
});

test('invalid stored theme falls back to dark', async () => {
  await seed('console', 'neon');
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'default', scope: 'console', color: '#070B12' });
});

test('choosing the applied default repairs an invalid stored theme', async () => {
  await seed('console', 'neon');
  assert.equal((await attrs()).pack, 'default');
  await cdp.eval(`window.LightGuardPrototype.setTheme('console', 'dark')`);
  assert.equal(await cdp.eval(`localStorage.getItem('lightguard.theme.console')`), 'dark');
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'default', scope: 'console', color: '#070B12' });
});

test('storage failure keeps in-session theming and falls back after reload', async () => {
  await reset();
  const script = await cdp.addInitScript(`
    Storage.prototype.getItem = function () { throw new Error('blocked'); };
    Storage.prototype.setItem = function () { throw new Error('blocked'); };
  `);
  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.app')`);
  await cdp.eval(`window.LightGuardPrototype.setTheme('console', 'tech')`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'tech'`);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('#view .settings-grid')`);
  assert.equal((await attrs()).pack, 'default');
  await cdp.removeInitScript(script);
  await cdp.reload();
});

test('backup validation keeps current scope and reports unknown ids', async () => {
  await reset();
  const valid = await cdp.eval(`window.LightGuardPrototype.validateBackupThemes({ console: 'claude', desktop: 'tech' })`);
  assert.deepEqual(valid, { themes: { console: 'claude', desktop: 'tech' }, warnings: [] });

  const unknown = await cdp.eval(`window.LightGuardPrototype.validateBackupThemes({ console: 'pixel', desktop: 'pixel-light' })`);
  assert.deepEqual(unknown.themes, { console: 'pixel', desktop: 'dark' });
  assert.match(unknown.warnings[0], /未知主题：pixel-light，已保留当前主题/);

  const missing = await cdp.eval(`window.LightGuardPrototype.validateBackupThemes(undefined)`);
  assert.deepEqual(missing, { themes: { console: 'dark', desktop: 'dark' }, warnings: [] });

  const bad = await cdp.eval(`window.LightGuardPrototype.validateBackupThemes(null)`);
  assert.deepEqual(bad.themes, { console: 'dark', desktop: 'dark' });
  assert.equal(bad.warnings.length, 1);
});

test('settings exposes one seven-option radiogroup per scope', async () => {
  await reset('#/settings');
  await cdp.waitFor(`!!document.querySelector('[data-theme-group="console"] input[data-theme-option]')`);
  const groups = await cdp.eval(`Array.from(document.querySelectorAll('[data-theme-group]')).map((group) => ({
    scope: group.dataset.themeGroup,
    role: group.querySelector('[role="radiogroup"]')?.getAttribute('aria-label'),
    count: group.querySelectorAll('input[data-theme-option]').length,
    values: Array.from(group.querySelectorAll('input[data-theme-option]')).map((input) => input.value),
    checked: Array.from(group.querySelectorAll('input[data-theme-option]')).filter((input) => input.checked).map((input) => input.value),
  }))`);
  assert.deepEqual(groups, [
    { scope: 'console', role: '控制台主题', count: 7, values: ['dark', 'light', 'system', 'claude', 'pixel', 'nes', 'tech'], checked: ['dark'] },
    { scope: 'desktop', role: '服务器桌面主题', count: 7, values: ['dark', 'light', 'system', 'claude', 'pixel', 'nes', 'tech'], checked: ['dark'] },
  ]);
  const current = await cdp.eval(`document.querySelector('[data-theme-current="console"]').textContent`);
  assert.equal(current, '深色');
});

test('desktop group saves preference without changing the settings page', async () => {
  await reset('#/settings');
  await cdp.waitFor(`!!document.querySelector('input[data-theme-option][data-theme-scope="desktop"][value="nes"]')`);
  await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="desktop"][value="nes"]').click()`);
  await cdp.waitFor(`document.querySelector('[data-theme-current="desktop"]').textContent.includes('红白机')`);
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'default', scope: 'console', color: '#070B12' });
  assert.equal(await cdp.eval(`localStorage.getItem('lightguard.theme.desktop')`), 'nes');
  const toastText = await cdp.eval(`document.querySelector('[data-toast-key="theme"] .toast-desc')?.textContent`);
  assert.equal(toastText, '当前为红白机（浅色），控制台外观不受影响');

  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('#view .settings-grid')`);
  assert.deepEqual(await attrs(), { theme: 'dark', pack: 'default', scope: 'console', color: '#070B12' });
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-current="desktop"]').textContent`), '红白机（浅色）');
});

test('setting the console theme updates the current line and radio state', async () => {
  await reset('#/settings');
  await cdp.waitFor(`!!document.querySelector('input[data-theme-option][data-theme-scope="console"][value="claude"]')`);
  await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="claude"]').click()`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'claude'`);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-current="console"]').textContent`), '纸墨（浅色）');
  assert.equal(await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="claude"]').checked`), true);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-current="desktop"]').textContent`), '深色');
});

test('arrow keys, Home and End move and select within a radiogroup', async () => {
  await reset('#/settings');
  await cdp.waitFor(`!!document.querySelector('input[data-theme-option][data-theme-scope="console"][value="dark"]')`);
  await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="dark"]').focus()`);
  await cdp.press('ArrowRight', 'ArrowRight', 39);
  await cdp.waitFor(`document.documentElement.dataset.theme === 'light'`);
  assert.equal(await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="light"]').checked`), true);
  await cdp.press('End', 'End', 35);
  await cdp.waitFor(`document.documentElement.dataset.theme === 'dark' && document.documentElement.dataset.themePack === 'tech'`);
  assert.equal(await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="tech"]').checked`), true);
  await cdp.press('Home', 'Home', 36);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'default'`);
  assert.equal(await cdp.eval(`document.querySelector('input[data-theme-option][data-theme-scope="console"][value="dark"]').checked`), true);
});

test('invalid stored theme shows a fallback note without failing settings', async () => {
  await seed('console', 'neon');
  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('[data-theme-fallback="console"]')`);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-fallback="console"]').hidden`), false);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-fallback="console"]').textContent.trim()`), '未识别的主题已回退为深色');
});

test('clicking the already-applied fallback card clears the warning and repairs storage', async () => {
  await seed('console', 'neon');
  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.settings-grid')`);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-fallback="console"]').hidden`), false);
  await cdp.eval(`document.querySelector('label.theme-option input[data-theme-option][value="dark"][data-theme-scope="console"]').closest('label').click()`);
  assert.equal(await cdp.eval(`document.querySelector('[data-theme-fallback="console"]').hidden`), true);
  assert.equal(await cdp.eval(`localStorage.getItem('lightguard.theme.console')`), 'dark');
});

test('console theme menu opens, stays open after selection and restores focus on Escape', async () => {
  await reset('#/overview');
  await cdp.eval(`document.querySelector('#themeToggle').click()`);
  await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
  assert.equal(await cdp.eval(`document.querySelectorAll('#console-theme-menu [data-theme-menu-option]').length`), 7);
  assert.equal(await cdp.eval(`document.querySelector('#themeToggle').getAttribute('aria-expanded')`), 'true');
  assert.equal(await cdp.eval(`document.querySelector('#console-theme-menu').getAttribute('aria-modal')`), 'false');

  await cdp.eval(`document.querySelector('#console-theme-menu [data-theme-choice="pixel"]').click()`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'pixel'`);
  assert.equal(await cdp.eval(`!!document.querySelector('#console-theme-menu')`), true);
  assert.equal(await cdp.eval(`document.querySelector('#console-theme-menu [data-theme-choice="pixel"]').getAttribute('aria-pressed')`), 'true');

  await cdp.eval(`document.querySelector('#console-theme-menu [data-theme-choice="pixel"]').focus()`);
  await cdp.press('Escape', 'Escape', 27);
  await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);
  assert.equal(await cdp.eval(`document.activeElement.id`), 'themeToggle');
});

test('clicking outside closes the console theme menu', async () => {
  await reset('#/overview');
  await cdp.eval(`document.querySelector('#themeToggle').click()`);
  await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
  await cdp.click(10, 500);
  await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);
  assert.equal(await cdp.eval(`document.querySelector('#themeToggle').getAttribute('aria-expanded')`), 'false');
});

test('mobile console menu is a modal sheet with a focus trap', async () => {
  await cdp.setViewport(390, 844);
  await reset('#/overview');
  await cdp.eval(`document.querySelector('#themeToggle').click()`);
  await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
  assert.equal(await cdp.eval(`document.querySelector('#console-theme-menu').getAttribute('aria-modal')`), 'true');
  const bounds = await cdp.eval(`(() => {
    const panel = document.querySelector('#console-theme-menu');
    const rect = panel.getBoundingClientRect();
    return { top: rect.top, bottom: rect.bottom, height: innerHeight, backdrop: getComputedStyle(document.querySelector('.theme-menu-backdrop')).display };
  })()`);
  assert.equal(bounds.backdrop, 'block');
  assert.ok(bounds.bottom <= bounds.height + 0.5);
  assert.ok(bounds.top >= bounds.height * 0.18 - 1);

  await cdp.eval(`(() => {
    const panel = document.querySelector('#console-theme-menu');
    const list = [...panel.querySelectorAll('button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])')].filter((el) => el.offsetParent !== null);
    window.__lgTrap = { first: list[0], last: list[list.length - 1] };
    window.__lgTrap.last.focus();
  })()`);
  await cdp.press('Tab', 'Tab', 9);
  assert.equal(await cdp.eval(`document.activeElement === window.__lgTrap.first`), true);
  await cdp.press('Tab', 'Tab', 9, 8);
  assert.equal(await cdp.eval(`document.activeElement === window.__lgTrap.last`), true);
  await cdp.press('Escape', 'Escape', 27);
  await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);
  assert.equal(await cdp.eval(`document.activeElement.id`), 'themeToggle');
  await cdp.setViewport(1440, 900);
});

test('desktop taskbar opens the desktop theme menu and keeps console theme unchanged', async () => {
  await reset();
  await cdp.eval(`
    localStorage.setItem('lightguard.theme.console', 'claude');
    localStorage.setItem('lightguard.theme.desktop', 'dark');
  `);
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);

  await cdp.eval(`document.querySelector('.desktop-taskbar [data-action="open-theme-menu"]').click()`);
  await cdp.waitFor(`!!document.querySelector('#desktop-theme-menu')`);
  assert.equal(await cdp.eval(`document.querySelectorAll('#desktop-theme-menu [data-theme-menu-option]').length`), 7);
  assert.equal(await cdp.eval(`document.querySelector('#desktop-theme-menu').getAttribute('aria-modal')`), 'false');

  await cdp.eval(`document.querySelector('#desktop-theme-menu [data-theme-choice="tech"]').click()`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'tech'`);
  assert.equal(await cdp.eval(`document.querySelector('.desktop-taskbar [data-theme-glyph="desktop"] use').getAttribute('href')`), '#i-spark');
  assert.equal(await cdp.eval(`!!document.querySelector('.conn-bar')`), true);

  await cdp.navigate(url('#/overview'));
  await cdp.waitFor(`!!document.querySelector('.app')`);
  assert.deepEqual(await attrs(), { theme: 'light', pack: 'claude', scope: 'console', color: '#F5F1E8' });
});

test('start menu footer opens the same desktop theme panel without closing the start menu', async () => {
  await reset();
  await cdp.eval(`localStorage.setItem('lightguard.theme.desktop', 'dark')`);
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  await cdp.eval(`document.querySelector('#tbStart').click()`);
  await cdp.waitFor(`!document.querySelector('#startMenu').hidden`);
  await cdp.eval(`document.querySelector('#startMenu [data-action="open-theme-menu"]').click()`);
  await cdp.waitFor(`!!document.querySelector('#desktop-theme-menu')`);
  assert.equal(await cdp.eval(`document.querySelector('#startMenu').hidden`), false);
  await cdp.eval(`document.querySelector('#desktop-theme-menu [data-theme-choice="nes"]').click()`);
  await cdp.waitFor(`document.documentElement.dataset.themePack === 'nes'`);
  assert.equal(await cdp.eval(`document.querySelector('#startMenu').hidden`), false);
});

test('default tokens preserve the existing appearance', async () => {
  await reset();
  const t = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    const v = (name) => s.getPropertyValue(name).trim();
    const dot = document.createElement('span');
    dot.className = 'status-dot';
    document.body.append(dot);
    const dotBg = getComputedStyle(dot).backgroundColor;
    dot.remove();
    return {
      bg: v('--bg'),
      success: v('--success'),
      icon: v('--icon-stroke'),
      gridSize: v('--app-grid-size'),
      bgImage: v('--app-bg-image'),
      gridImage: v('--app-grid-image'),
      shadowHard: v('--shadow-hard'),
      borderWidth: v('--border-width'),
      dotBg,
      bodyGridOpacity: getComputedStyle(document.body, '::after').opacity,
      iconStroke: getComputedStyle(document.querySelector('.icon')).strokeWidth,
    };
  })()`);
  assert.equal(t.bg, '#070b12');
  assert.equal(t.success.toLowerCase(), '#3ddc97');
  assert.equal(t.icon, '1.7px');
  assert.equal(t.gridSize, '56px 56px');
  assert.match(t.bgImage, /61, 220, 151/);
  assert.match(t.gridImage, /148, 163, 184/);
  assert.equal(t.shadowHard, 'none');
  assert.equal(t.borderWidth, '1px');
  assert.equal(t.dotBg, 'rgb(61, 220, 151)');
  assert.equal(t.bodyGridOpacity, '0.32');
  assert.equal(t.iconStroke, '1.7px');

  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  const d = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    return {
      blur: s.getPropertyValue('--panel-blur').trim(),
      grid: s.getPropertyValue('--app-grid-size').trim(),
      gridColor: s.getPropertyValue('--app-grid-color').trim(),
      wallpaper: getComputedStyle(document.querySelector('.desktop-bg')).backgroundImage,
      vignette: getComputedStyle(document.querySelector('.desktop-shell'), '::before').backgroundImage,
      conn: getComputedStyle(document.querySelector('.conn-bar')).backdropFilter,
    };
  })()`);
  assert.equal(d.blur, '16px');
  assert.equal(d.grid, '72px 72px');
  assert.equal(d.gridColor, 'rgba(148, 163, 184, 0.05)');
  assert.match(d.wallpaper, /5, 10, 17/);
  assert.equal(d.vignette, 'none');
  assert.equal(d.conn, 'blur(16px)');
  assert.deepEqual(cdp.pageErrors, []);
});

test('pixel font is local, loadable, and within budget', async () => {
  const file = `${PROTO}/fonts/lg-pixel-12px-subset.woff2`;
  const size = statSync(file).size;
  assert.ok(size > 0 && size <= 80 * 1024, `font size ${size} bytes`);

  await reset();
  const loaded = await cdp.eval(`document.fonts.load('16px "LG Pixel"').then((faces) => faces.length)`);
  assert.ok(loaded > 0);
  assert.equal(await cdp.eval(`document.fonts.check('16px "LG Pixel"')`), true);

  const external = cdp.requests.filter((u) => !u.startsWith(origin) && !u.startsWith('data:'));
  assert.deepEqual(external, []);
  assert.equal(await cdp.eval(`Array.from(document.styleSheets).some((sheet) => {
    try {
      return Array.from(sheet.cssRules).some((rule) => rule.cssText.includes('lg-pixel-12px-subset.woff2'));
    } catch { return false; }
  })`), true);
});

test('claude pack applies paper tokens and component details', async () => {
  await seed('console', 'claude');
  const t = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    const v = (name) => s.getPropertyValue(name).trim();
    const h1 = getComputedStyle(document.querySelector('.page-head h1'));
    const card = getComputedStyle(document.querySelector('.stat-card'));
    const texture = getComputedStyle(document.querySelector('.stat-card'), '::before');
    return {
      bg: v('--bg'),
      primary: v('--primary'),
      radius: v('--radius-lg'),
      grid: v('--app-grid-image'),
      bgImage: v('--app-bg-image'),
      font: h1.fontFamily,
      cardRadius: card.borderRadius,
      texture: texture.backgroundImage,
      textureOpacity: texture.opacity,
    };
  })()`);
  assert.equal(t.bg, '#F5F1E8');
  assert.equal(t.primary, '#A94B24');
  assert.equal(t.radius, '18px');
  assert.equal(t.grid, 'none');
  assert.match(t.bgImage, /169, 75, 36/);
  assert.match(t.font, /Iowan Old Style|Palatino|Songti SC|Georgia/);
  assert.equal(t.cardRadius, '18px');
  assert.match(t.texture, /radial-gradient/);
  assert.equal(t.textureOpacity, '0.5');

  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.settings-card-head h3')`);
  assert.match(await cdp.eval(`getComputedStyle(document.querySelector('.settings-card-head h3')).fontFamily`), /Iowan Old Style|Palatino|Georgia/);
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.theme-option')).borderColor`), 'rgba(74, 60, 47, 0.6)');

  await seed('desktop', 'claude');
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  const d = await cdp.eval(`({
    wallpaper: getComputedStyle(document.querySelector('.desktop-bg')).backgroundImage,
    vignette: getComputedStyle(document.querySelector('.desktop-shell'), '::before').backgroundImage,
    taskbar: getComputedStyle(document.querySelector('.desktop-taskbar')).backdropFilter,
    conn: getComputedStyle(document.querySelector('.conn-bar')).backdropFilter,
    glow: getComputedStyle(document.querySelector('.desktop-bg'), '::after').display,
  })`);
  assert.match(d.wallpaper, /247, 242, 232/);
  assert.match(d.vignette, /74, 60, 47/);
  assert.equal(d.taskbar, 'blur(10px) saturate(1.25)');
  assert.equal(d.conn, 'blur(10px)');
  assert.equal(d.glow, 'none');
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.desktop-brand')).opacity`), '1');
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.desktop-hint')).backgroundColor`), 'rgba(255, 253, 247, 0.86)');
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.desktop-hint')).color`), 'rgb(90, 81, 72)');
  assert.deepEqual(cdp.pageErrors, []);
});

test('pixel pack applies hard edges, pixel font, and square controls', async () => {
  await seed('console', 'pixel');
  await cdp.eval(`document.fonts.ready`);
  const t = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    const v = (name) => s.getPropertyValue(name).trim();
    const h1 = getComputedStyle(document.querySelector('.page-head h1'));
    const body = getComputedStyle(document.body);
    const card = getComputedStyle(document.querySelector('.stat-card'));
    const btn = getComputedStyle(document.querySelector('.btn'));
    return {
      bg: v('--bg'),
      radius: v('--radius-md'),
      border: v('--border-width'),
      hard: v('--shadow-hard'),
      icon: v('--icon-stroke'),
      grid: v('--app-grid-size'),
      gridOpacity: getComputedStyle(document.body, '::after').opacity,
      font: h1.fontFamily,
      bodyFont: body.fontFamily,
      cardRadius: card.borderRadius,
      cardBorder: card.borderWidth,
      btnShadow: btn.boxShadow,
      texture: getComputedStyle(document.querySelector('.stat-card'), '::before').backgroundImage,
      dotRadius: getComputedStyle((() => {
        const dot = document.createElement('span');
        dot.className = 'status-dot';
        document.body.append(dot);
        return dot;
      })()).borderRadius,
    };
  })()`);
  assert.equal(t.bg, '#10131C');
  assert.equal(t.radius, '0');
  assert.equal(t.border, '2px');
  assert.match(t.hard, /4px 4px 0/);
  assert.equal(t.icon, '2px');
  assert.equal(t.grid, '16px 16px');
  assert.equal(t.gridOpacity, '1');
  assert.match(t.font, /LG Pixel/);
  assert.doesNotMatch(t.bodyFont, /LG Pixel/);
  assert.equal(t.cardRadius, '0px');
  assert.equal(t.cardBorder, '2px');
  assert.match(t.btnShadow, /4px 4px 0px/);
  assert.match(t.texture, /conic-gradient/);
  assert.match(await cdp.eval(`getComputedStyle(document.querySelector('.server-name')).fontFamily`), /LG Pixel/);
  assert.equal(await cdp.eval(`document.fonts.check('16px "LG Pixel"')`), true);
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.status-pill')).borderRadius`), '0px');

  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.settings-card-head h3')`);
  assert.match(await cdp.eval(`getComputedStyle(document.querySelector('.settings-card-head h3')).fontFamily`), /LG Pixel/);
  assert.equal(await cdp.eval(`getComputedStyle(document.querySelector('.theme-option')).borderColor`), 'rgb(113, 130, 176)');

  await seed('desktop', 'pixel');
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  await cdp.waitFor(`!!document.querySelector('.os-window')`);
  const d = await cdp.eval(`({
    taskbar: getComputedStyle(document.querySelector('.desktop-taskbar')).backdropFilter,
    conn: getComputedStyle(document.querySelector('.conn-bar')).backdropFilter,
    wallpaper: getComputedStyle(document.querySelector('.desktop-bg')).backgroundImage,
    windowRadius: getComputedStyle(document.querySelector('.os-window')).borderRadius,
    brandOpacity: getComputedStyle(document.querySelector('.desktop-brand')).opacity,
    brandColor: getComputedStyle(document.querySelector('.desktop-brand')).color,
    hintBackground: getComputedStyle(document.querySelector('.desktop-hint')).backgroundColor,
    hintFilter: getComputedStyle(document.querySelector('.desktop-hint')).backdropFilter,
    glow: getComputedStyle(document.querySelector('.desktop-bg'), '::after').display,
  })`);
  assert.equal(d.taskbar, 'none');
  assert.equal(d.conn, 'none');
  assert.equal(d.hintFilter, 'none');
  assert.equal(d.glow, 'none');
  assert.match(d.wallpaper, /11, 13, 20/);
  assert.equal(d.windowRadius, '0px');
  assert.equal(d.brandOpacity, '1');
  assert.equal(d.brandColor, 'rgba(195, 204, 230, 0.7)');
  assert.equal(d.hintBackground, 'rgba(5, 10, 17, 0.38)');
  assert.deepEqual(cdp.pageErrors, []);
});

test('nes pack applies toy-plastic edges and keeps status readable', async () => {
  await seed('console', 'nes');
  await cdp.eval(`document.fonts.ready`);
  const t = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    const v = (name) => s.getPropertyValue(name).trim();
    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
    const danger = document.createElement('button');
    danger.className = 'btn btn-danger';
    danger.textContent = '删除';
    const pill = document.createElement('span');
    pill.className = 'pill pill-online';
    pill.textContent = '在线';
    const statusPill = document.createElement('span');
    statusPill.className = 'status-pill online';
    statusPill.textContent = '在线';
    document.body.append(danger, pill, statusPill);
    const result = {
      bg: v('--bg'),
      primary: v('--primary'),
      radius: v('--radius-md'),
      border: v('--border-width'),
      font: style('.page-head h1').fontFamily,
      serverFont: style('.server-name').fontFamily,
      statFont: style('.stat-value').fontFamily,
      texture: style('.stat-card', '::before').backgroundImage,
      cardRadius: style('.stat-card').borderRadius,
      cardBorder: style('.stat-card').borderWidth,
      cardBorderColor: style('.stat-card').borderTopColor,
      cardShadow: style('.stat-card').boxShadow,
      dangerBg: getComputedStyle(danger).backgroundColor,
      dangerColor: getComputedStyle(danger).color,
      dangerBorder: getComputedStyle(danger).borderTopColor,
      pillBg: getComputedStyle(pill).backgroundColor,
      pillColor: getComputedStyle(pill).color,
      pillRadius: getComputedStyle(pill).borderRadius,
      statusPillBg: getComputedStyle(statusPill).backgroundColor,
      statusPillColor: getComputedStyle(statusPill).color,
      statusPillRadius: getComputedStyle(statusPill).borderRadius,
      statusPillBorder: getComputedStyle(statusPill).borderTopColor,
      realPillBg: style('.status-pill.online').backgroundColor,
      realPillColor: style('.status-pill.online').color,
      realPillRadius: style('.status-pill.online').borderRadius,
      tagRadius: style('.tag').borderRadius,
      kbdRadius: style('.search-kbd').borderRadius,
    };
    danger.remove();
    pill.remove();
    statusPill.remove();
    return result;
  })()`);
  assert.equal(t.bg, '#EDEAE3');
  assert.equal(t.primary, '#C8102E');
  assert.equal(t.radius, '0');
  assert.equal(t.border, '2px');
  assert.match(t.font, /LG Pixel/);
  assert.match(t.serverFont, /LG Pixel/);
  assert.match(t.statFont, /LG Pixel/);
  assert.match(t.texture, /repeating-linear-gradient/);
  assert.equal(t.cardRadius, '0px');
  assert.equal(t.cardBorder, '2px');
  assert.equal(t.cardBorderColor, 'rgb(43, 43, 43)');
  assert.match(t.cardShadow, /4px 4px 0px/);
  assert.equal(t.dangerBg, 'rgb(27, 27, 27)');
  assert.equal(t.dangerColor, 'rgb(255, 154, 166)');
  assert.equal(t.dangerBorder, 'rgb(200, 16, 46)');
  assert.equal(t.pillBg, 'rgb(46, 107, 58)');
  assert.equal(t.pillColor, 'rgb(255, 255, 255)');
  assert.equal(t.pillRadius, '0px');
  assert.equal(t.statusPillBg, 'rgb(46, 107, 58)');
  assert.equal(t.statusPillColor, 'rgb(255, 255, 255)');
  assert.equal(t.statusPillRadius, '0px');
  assert.equal(t.statusPillBorder, 'rgb(43, 43, 43)');
  assert.equal(t.realPillBg, 'rgb(46, 107, 58)');
  assert.equal(t.realPillColor, 'rgb(255, 255, 255)');
  assert.equal(t.realPillRadius, '0px');
  assert.equal(t.tagRadius, '0px');
  assert.equal(t.kbdRadius, '0px');

  await cdp.navigate(url('#/connections'));
  await cdp.waitFor(`!!document.querySelector('.chip')`);
  const c = await cdp.eval(`(() => {
    const style = (selector) => getComputedStyle(document.querySelector(selector));
    return {
      chipRadius: style('.chip').borderRadius,
      countRadius: style('.chip-count').borderRadius,
      searchRadius: style('.search-box input').borderRadius,
      searchBorder: style('.search-box input').borderTopWidth,
      searchBorderColor: style('.search-box input').borderTopColor,
    };
  })()`);
  assert.equal(c.chipRadius, '0px');
  assert.equal(c.countRadius, '0px');
  assert.equal(c.searchRadius, '0px');
  assert.equal(c.searchBorder, '2px');
  assert.equal(c.searchBorderColor, 'rgb(17, 17, 17)');

  await cdp.eval(`(() => {
    const input = document.querySelector('#logSearch');
    input.value = '__no_such_log__';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await cdp.waitFor(`!!document.querySelector('.empty h3')`);
  const e = await cdp.eval(`({
    font: getComputedStyle(document.querySelector('.empty h3')).fontFamily,
    radius: getComputedStyle(document.querySelector('.empty')).borderRadius,
  })`);
  assert.match(e.font, /LG Pixel/);
  assert.equal(e.radius, '0px');

  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.settings-card-head h3')`);
  await cdp.eval(`document.querySelector('#themeToggle').click()`);
  await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
  const m = await cdp.eval(`(() => {
    const style = (selector) => getComputedStyle(document.querySelector(selector));
    return {
      headingFont: style('.settings-card-head h3').fontFamily,
      optionBorder: style('.theme-option').borderTopWidth,
      optionBorderColor: style('.theme-option').borderTopColor,
      optionRadius: style('.theme-option').borderRadius,
      badgeRadius: style('.theme-scope-badge').borderRadius,
      menuRadius: style('.theme-menu').borderRadius,
      menuOptionRadius: style('.theme-menu-option').borderRadius,
      menuOptionBorder: style('.theme-menu-option').borderTopWidth,
      menuOptionBorderColor: style('.theme-menu-option').borderTopColor,
    };
  })()`);
  assert.match(m.headingFont, /LG Pixel/);
  assert.equal(m.optionBorder, '2px');
  assert.equal(m.optionBorderColor, 'rgb(43, 43, 43)');
  assert.equal(m.optionRadius, '0px');
  assert.equal(m.badgeRadius, '0px');
  assert.equal(m.menuRadius, '0px');
  assert.equal(m.menuOptionRadius, '0px');
  assert.equal(m.menuOptionBorder, '2px');
  assert.equal(m.menuOptionBorderColor, 'rgb(17, 17, 17)');

  await cdp.press('Escape', 'Escape', 27);
  await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);
  const point = await cdp.eval(`(() => {
    const el = document.querySelector('.theme-option:not(.active)');
    el.scrollIntoView({ block: 'center' });
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  })()`);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: point.x, y: point.y, button: 'none' });
  await cdp.waitFor(
    `getComputedStyle(document.querySelector('.theme-option:not(.active)')).borderTopColor === 'rgb(17, 17, 17)'`,
  );
  const hover = await cdp.eval(`(() => {
    const el = document.querySelector('.theme-option:not(.active)');
    return { hovered: el.matches(':hover'), border: getComputedStyle(el).borderTopColor };
  })()`);
  assert.equal(hover.hovered, true);
  assert.equal(hover.border, 'rgb(17, 17, 17)');

  await seed('desktop', 'nes');
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  await cdp.waitFor(`!!document.querySelector('.os-window')`);
  const d = await cdp.eval(`(() => {
    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
    const shell = getComputedStyle(document.querySelector('.desktop-shell'));
    return {
      taskbar: style('.desktop-taskbar').backdropFilter,
      taskbarBorder: style('.desktop-taskbar').borderTopColor,
      taskbarWidth: style('.desktop-taskbar').borderTopWidth,
      conn: style('.conn-bar').backdropFilter,
      hintFilter: style('.desktop-hint').backdropFilter,
      hintBg: style('.desktop-hint').backgroundColor,
      hintColor: style('.desktop-hint').color,
      hintRadius: style('.desktop-hint').borderRadius,
      brandOpacity: style('.desktop-brand').opacity,
      brandColor: style('.desktop-brand').color,
      text: shell.getPropertyValue('--desktop-text').trim(),
      textMuted: shell.getPropertyValue('--desktop-text-muted').trim(),
      wallpaper: style('.desktop-bg').backgroundImage,
      glow: style('.desktop-bg', '::after').display,
      windowRadius: style('.os-window').borderRadius,
      windowShadow: style('.os-window').boxShadow,
      titlebarBg: style('.os-titlebar').backgroundColor,
      titlebarBorder: style('.os-titlebar').borderBottomColor,
      trayRadius: style('.os-tray-panel').borderRadius,
    };
  })()`);
  assert.equal(d.taskbar, 'none');
  assert.equal(d.taskbarBorder, 'rgb(200, 16, 46)');
  assert.equal(d.taskbarWidth, '3px');
  assert.equal(d.conn, 'none');
  assert.equal(d.hintFilter, 'none');
  assert.equal(d.hintBg, 'rgba(247, 245, 239, 0.94)');
  assert.equal(d.hintColor, 'rgb(63, 61, 56)');
  assert.equal(d.hintRadius, '0px');
  assert.equal(d.brandOpacity, '1');
  assert.equal(d.brandColor, 'rgb(63, 61, 56)');
  assert.equal(d.text, '#1B1B1B');
  assert.equal(d.textMuted, 'rgba(27, 27, 27, 0.65)');
  assert.match(d.wallpaper, /243, 240, 233/);
  assert.equal(d.glow, 'none');
  assert.equal(d.windowRadius, '0px');
  assert.match(d.windowShadow, /8px 8px 0px/);
  assert.equal(d.titlebarBg, 'rgb(251, 250, 245)');
  assert.equal(d.titlebarBorder, 'rgb(43, 43, 43)');
  assert.equal(d.trayRadius, '0px');
  assert.deepEqual(cdp.pageErrors, []);
});

test('pixel and nes square every control and reset nested light preview structure', async () => {
  const consoleSelectors = [
    '.nav-item', '.fleet-chip', '.icon-btn', '.search-kbd', '.chip-count',
    '.status-pill', '.server-card', '.search-box input', '.segmented',
    '.brand-mark', '.nav-badge', '.form-preview', '.user-card', '.avatar',
    '.status-dot', '.stat-icon', '.mini-bar', '.error-strip', '.warning-strip',
  ];
  for (const pack of ['pixel', 'nes']) {
    await seed('console', pack);
    const base = await cdp.eval(`(() => {
      const selectors = ${JSON.stringify(consoleSelectors)};
      const radii = Object.fromEntries(selectors.map((selector) => {
        const el = document.querySelector(selector);
        return [selector, el ? getComputedStyle(el).borderRadius : null];
      }));
      return {
        radii,
        serverBorder: getComputedStyle(document.querySelector('.server-card')).borderWidth,
        cardBorder: getComputedStyle(document.querySelector('.stat-card')).borderWidth,
        formPreviewBorder: getComputedStyle(document.querySelector('.form-preview')).borderWidth,
      };
    })()`);
    for (const [selector, radius] of Object.entries(base.radii)) {
      assert.notEqual(radius, null, `${pack} ${selector} missing`);
      assert.equal(radius, '0px', `${pack} ${selector}`);
    }
    assert.equal(base.serverBorder, '2px', `${pack} server-card border`);
    assert.equal(base.cardBorder, '2px', `${pack} stat-card border`);
    assert.equal(base.formPreviewBorder, '2px', `${pack} form-preview border`);

    await cdp.navigate(url('#/settings'));
    await cdp.waitFor(`!!document.querySelector('.theme-thumb[data-theme-for="light"]')`);
    const settings = await cdp.eval(`(() => {
      const radius = (selector) => getComputedStyle(document.querySelector(selector)).borderRadius;
      const light = document.querySelector('.theme-thumb[data-theme-for="light"][data-theme-scope="console"]');
      const ls = getComputedStyle(light);
      return {
        badge: radius('.theme-scope-badge'),
        pill: radius('.pill'),
        optionCheck: radius('.theme-option-check'),
        settingsIcon: radius('.settings-icon'),
        themeNote: radius('.theme-note'),
        toggle: radius('.switch'),
        toggleKnob: getComputedStyle(document.querySelector('.switch'), '::after').borderRadius,
        previewFrame: ls.borderRadius,
        previewBorder: ls.borderWidth,
        previewCard: getComputedStyle(light.querySelector('.tt-card-a')).borderRadius,
        lightRadius: ls.getPropertyValue('--radius-sm').trim(),
        lightBorder: ls.getPropertyValue('--border-width').trim(),
        lightFont: ls.getPropertyValue('--font-display').trim(),
        lightTexture: ls.getPropertyValue('--surface-texture').trim(),
        lightVignette: ls.getPropertyValue('--desktop-vignette').trim(),
      };
    })()`);
    assert.equal(settings.badge, '0px', `${pack} theme-scope-badge`);
    assert.equal(settings.pill, '0px', `${pack} pill`);
    assert.equal(settings.optionCheck, '0px', `${pack} theme-option-check`);
    assert.equal(settings.settingsIcon, '0px', `${pack} settings-icon`);
    assert.equal(settings.themeNote, '0px', `${pack} theme-note`);
    assert.equal(settings.toggle, '0px', `${pack} switch`);
    assert.equal(settings.toggleKnob, '0px', `${pack} switch knob`);
    assert.equal(settings.previewFrame, '6px', `${pack} light preview frame radius`);
    assert.equal(settings.previewBorder, '1px', `${pack} light preview frame border`);
    assert.equal(settings.previewCard, '5px', `${pack} light preview card radius`);
    assert.equal(settings.lightRadius, '8px', `${pack} nested light --radius-sm`);
    assert.equal(settings.lightBorder, '1px', `${pack} nested light --border-width`);
    assert.doesNotMatch(settings.lightFont, /LG Pixel/, `${pack} nested light display font`);
    assert.equal(settings.lightTexture, 'none', `${pack} nested light texture`);
    assert.equal(settings.lightVignette, 'none', `${pack} nested light vignette`);

    await seed('desktop', pack);
    await cdp.navigate(url('#/desktop/prod-web-01'));
    await cdp.waitFor(`!!document.querySelector('.desktop-icon')`);
    await cdp.waitFor(`!document.querySelector('#desktopBoot')`, 10000);
    const desktop = await cdp.eval(`(() => {
      const g = (selector) => getComputedStyle(document.querySelector(selector));
      return {
        search: g('.tb-search').borderRadius,
        searchBorder: g('.tb-search').borderWidth,
        tbApp: g('.tb-app').borderRadius,
        tbTile: g('.tb-app .icon-tile').borderRadius,
        icon: g('.desktop-icon').borderRadius,
        tile: g('.desktop-icon .icon-tile').borderRadius,
        startTile: g('.start-app .icon-tile').borderRadius,
        titleIcon: g('.os-title-icon').borderRadius,
        ctrl: g('.os-ctrl').borderRadius,
        tbStart: g('.tb-start').borderRadius,
        tbTray: g('.tb-tray-btn').borderRadius,
        tbClock: g('.tb-clock').borderRadius,
        avatar: g('.server-avatar').borderRadius,
        specTile: g('.spec-tile').borderRadius,
        bar: g('.bar').borderRadius,
        aboutHeroBorder: g('.about-hero').borderWidth,
        specTileBorder: g('.spec-tile').borderWidth,
        avatarBorder: g('.server-avatar').borderWidth,
        tileShadow: g('.desktop-icon .icon-tile').boxShadow,
        tbTileShadow: g('.tb-app .icon-tile').boxShadow,
        titleShadow: g('.os-title-icon').boxShadow,
      };
    })()`);
    for (const [key, value] of Object.entries(desktop)) {
      if (key === 'searchBorder' || key.endsWith('Border') || key.endsWith('Shadow')) continue;
      assert.equal(value, '0px', `${pack} desktop ${key}`);
    }
    assert.equal(desktop.searchBorder, '2px', `${pack} taskbar search border`);
    assert.equal(desktop.aboutHeroBorder, '2px', `${pack} about-hero border`);
    assert.equal(desktop.specTileBorder, '2px', `${pack} spec-tile border`);
    assert.equal(desktop.avatarBorder, '2px', `${pack} server-avatar border`);
    assert.match(desktop.tileShadow, /^rgba?\([^)]*\) 4px 4px 0px 0px$/, `${pack} desktop icon tile hard shadow`);
    assert.equal(desktop.tbTileShadow, 'none', `${pack} taskbar icon tile shadow`);
    assert.equal(desktop.titleShadow, 'none', `${pack} title icon shadow`);

    await cdp.eval('document.body.focus()');
    for (let i = 0; i < 3; i++) await cdp.press('Tab');
    const focus = await cdp.eval(`(() => {
      const el = document.activeElement;
      const s = getComputedStyle(el);
      const probe = document.createElement('span');
      probe.style.color = 'var(--focus)';
      document.body.append(probe);
      const expected = getComputedStyle(probe).color;
      const bgProbe = document.createElement('span');
      bgProbe.style.color = 'var(--bg)';
      document.body.append(bgProbe);
      const bg = getComputedStyle(bgProbe).color;
      probe.remove();
      bgProbe.remove();
      const rgb = (value) => value.match(/\\d+(\\.\\d+)?/g).slice(0, 3).map(Number);
      const luminance = (value) => {
        const [r, g, b] = rgb(value).map((c) => {
          const v = c / 255;
          return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
        });
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
      };
      const contrast = (a, b) => {
        const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
        return (hi + 0.05) / (lo + 0.05);
      };
      return {
        cls: el.className,
        isDesktopIcon: el.classList.contains('desktop-icon'),
        visible: el.matches(':focus-visible'),
        color: s.outlineColor,
        width: s.outlineWidth,
        style: s.outlineStyle,
        offset: s.outlineOffset,
        expected,
        contrast: contrast(s.outlineColor, bg),
      };
    })()`);
    assert.equal(focus.isDesktopIcon, true, `${pack} focused element`);
    assert.equal(focus.visible, true, `${pack} desktop icon focus-visible`);
    assert.equal(focus.color, focus.expected, `${pack} desktop icon focus color`);
    assert.equal(focus.width, '2px', `${pack} desktop icon focus width`);
    assert.equal(focus.style, 'solid', `${pack} desktop icon focus style`);
    assert.equal(focus.offset, '2px', `${pack} desktop icon focus offset`);
    assert.ok(focus.contrast >= 3, `${pack} desktop icon focus contrast ${focus.contrast.toFixed(2)}`);
    assert.deepEqual(cdp.pageErrors, []);
  }

  for (const id of ['dark', 'claude', 'tech']) {
    await seed('console', id);
    const geometry = await cdp.eval(`(() => {
      const g = (selector) => getComputedStyle(document.querySelector(selector));
      return {
        brand: g('.brand-mark').borderRadius,
        preview: g('.form-preview').borderRadius,
        previewBorder: g('.form-preview').borderWidth,
        statBorder: g('.stat-card').borderWidth,
      };
    })()`);
    assert.notEqual(geometry.brand, '0px', `${id} brand-mark not squared by pack rule`);
    assert.notEqual(geometry.preview, '0px', `${id} form-preview not squared by pack rule`);
    assert.equal(geometry.previewBorder, '1px', `${id} form-preview default border`);
    assert.equal(geometry.statBorder, '1px', `${id} stat-card default border`);
  }
});

test('pixel and nes square remaining cards on logs, modals, tray, start and app windows', async () => {
  for (const pack of ['pixel', 'nes']) {
    await seed('console', pack);
    await cdp.navigate(url('#/connections'));
    await cdp.waitFor(`!!document.querySelector('.log-row')`);
    let card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('.log-row'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} log-row radius`);
    assert.equal(card.border, '2px', `${pack} log-row border`);

    await cdp.eval(`document.querySelector('.log-row .log-main').click()`);
    await cdp.waitFor(`!!document.querySelector('.log-row.expanded .log-detail-inner')`);
    card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('.log-detail-inner'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} log-detail-inner radius`);
    assert.equal(card.border, '2px', `${pack} log-detail-inner border`);

    await cdp.navigate(url('#/overview'));
    await cdp.waitFor(`!!document.querySelector('.app')`);
    await cdp.eval(`document.querySelector('#fleetChip').click()`);
    await cdp.waitFor(`!!document.querySelector('.modal .about-item')`);
    card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('.modal .about-item'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} about-item radius`);
    assert.equal(card.border, '2px', `${pack} about-item border`);
    await cdp.press('Escape');
    await cdp.waitFor(`!document.querySelector('.modal')`);

    await cdp.eval(`document.querySelector('.window-bar [data-action="toggle-tray"]').click()`);
    await cdp.waitFor(`!document.querySelector('#trayPop').hidden`);
    card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('#trayPop'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} tray-pop radius`);
    assert.equal(card.border, '2px', `${pack} tray-pop border`);

    await cdp.eval(`(() => {
      document.querySelector('[data-action="open-wizard"]').click();
      const set = (id, value) => { document.getElementById(id).value = value; };
      set('wizName', 'theme-check');
      set('wizHost', '10.0.0.9');
      document.querySelector('[data-action="wizard-next"]').click();
    })()`);
    await cdp.waitFor(`!!document.querySelector('.modal .wizard-summary')`);
    const wizard = await cdp.eval(`(() => {
      const read = (selector) => {
        const s = getComputedStyle(document.querySelector(selector));
        return { radius: s.borderRadius, border: s.borderTopWidth };
      };
      return { summary: read('.wizard-summary'), auth: read('.auth-option') };
    })()`);
    assert.equal(wizard.summary.radius, '0px', `${pack} wizard-summary radius`);
    assert.equal(wizard.summary.border, '2px', `${pack} wizard-summary border`);
    assert.equal(wizard.auth.radius, '0px', `${pack} auth-option radius`);
    assert.equal(wizard.auth.border, '2px', `${pack} auth-option border`);

    await cdp.eval(`(() => {
      document.querySelector('[data-action="wizard-auth"][data-auth="password"]').click();
      document.getElementById('wizPassword').value = 'demo-password';
      document.querySelector('[data-action="wizard-next"]').click();
    })()`);
    await cdp.waitFor(`!!document.querySelector('.modal .probe')`);
    const probe = await cdp.eval(`(() => {
      const read = (selector) => {
        const s = getComputedStyle(document.querySelector(selector));
        return { radius: s.borderRadius, border: s.borderTopWidth };
      };
      return { box: read('.probe'), icon: read('.probe-icon') };
    })()`);
    assert.equal(probe.box.radius, '0px', `${pack} probe radius`);
    assert.equal(probe.box.border, '2px', `${pack} probe border`);
    assert.equal(probe.icon.radius, '0px', `${pack} probe-icon radius`);
    assert.equal(probe.icon.border, '2px', `${pack} probe-icon border`);
    await cdp.press('Escape');
    await cdp.waitFor(`!document.querySelector('.modal')`);

    await seed('desktop', pack);
    await cdp.navigate(url('#/desktop/prod-web-01'));
    await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
    await cdp.waitFor(`!document.querySelector('#desktopBoot')`, 10000);
    await cdp.eval(`document.querySelector('#tbStart').click()`);
    await cdp.waitFor(`!document.querySelector('#startMenu').hidden`);
    card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('.start-recent-item'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} start-recent-item radius`);
    assert.equal(card.border, '2px', `${pack} start-recent-item border`);

    const appSelectors = [
      ['performance', '.core-item'],
      ['disks', '.drive-card'],
      ['processes', '.proc-table'],
      ['connection', '.conn-app-hero'],
    ];
    for (const [app, selector] of appSelectors) {
      const visible = await cdp.eval(`!document.querySelector('#startMenu').hidden`);
      if (!visible) {
        await cdp.eval(`document.querySelector('#tbStart').click()`);
        await cdp.waitFor(`!document.querySelector('#startMenu').hidden`);
      }
      await cdp.eval(`document.querySelector('#startMenu [data-app="${app}"]').click()`);
      await cdp.waitFor(`!!document.querySelector('${selector}')`);
      card = await cdp.eval(`(() => {
        const s = getComputedStyle(document.querySelector('${selector}'));
        return { radius: s.borderRadius, border: s.borderTopWidth };
      })()`);
      assert.equal(card.radius, '0px', `${pack} ${selector} radius`);
      assert.equal(card.border, '2px', `${pack} ${selector} border`);
    }

    const menuVisible = await cdp.eval(`!document.querySelector('#startMenu').hidden`);
    if (!menuVisible) {
      await cdp.eval(`document.querySelector('#tbStart').click()`);
      await cdp.waitFor(`!document.querySelector('#startMenu').hidden`);
    }
    await cdp.eval(`document.querySelector('#startMenu [data-app="about"]').click()`);
    await cdp.waitFor(`!!document.querySelector('.os-app-nav button.active')`);
    const tabMark = await cdp.eval(`getComputedStyle(document.querySelector('.os-app-nav button.active'), '::before').borderRadius`);
    assert.equal(tabMark, '0px', `${pack} active app tab mark`);

    await cdp.eval(`document.querySelector('.desktop-taskbar [data-action="toggle-os-tray"]').click()`);
    await cdp.waitFor(`!!document.querySelector('.os-tray-kpi')`);
    card = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('.os-tray-kpi'));
      return { radius: s.borderRadius, border: s.borderTopWidth };
    })()`);
    assert.equal(card.radius, '0px', `${pack} os-tray-kpi radius`);
    assert.equal(card.border, '2px', `${pack} os-tray-kpi border`);
    assert.deepEqual(cdp.pageErrors, []);
  }
});

test('tech pack applies HUD tokens, brackets, and blur', async () => {
  await seed('console', 'tech');
  const t = await cdp.eval(`(() => {
    const s = getComputedStyle(document.documentElement);
    const v = (name) => s.getPropertyValue(name).trim();
    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
    const grid = getComputedStyle(document.body, '::after');
    const stat = style('.stat-value');
    return {
      bg: v('--bg'),
      surface: v('--surface'),
      surface2: v('--surface-2'),
      primary: v('--primary'),
      focus: v('--focus'),
      radius: v('--radius-md'),
      border: v('--border-width'),
      blur: v('--panel-blur'),
      gridSize: v('--app-grid-size'),
      gridImage: grid.backgroundImage,
      gridOpacity: grid.opacity,
      statFont: stat.fontFamily,
      statShadow: stat.textShadow,
      bodyShadow: style('.page-head p').textShadow,
      btnShadow: style('.btn-primary').boxShadow,
    };
  })()`);
  assert.equal(t.bg, '#050B14');
  assert.equal(t.surface, '#0A1624');
  assert.equal(t.surface2, '#0F2033');
  assert.equal(t.primary, '#00E5FF');
  assert.equal(t.focus, '#00E5FF');
  assert.equal(t.radius, '9px');
  assert.equal(t.border, '1px');
  assert.equal(t.blur, '20px');
  assert.equal(t.gridSize, '48px 48px');
  assert.match(t.gridImage, /rgba\(0, 229, 255, 0\.06\)/);
  assert.equal(t.gridOpacity, '0.9');
  assert.match(t.statFont, /JetBrains Mono|SFMono-Regular|Consolas/);
  assert.match(t.statShadow, /rgba\(0, 229, 255, 0\.18\)/);
  assert.equal(t.bodyShadow, 'none');

  // 发光只出现在主按钮悬停态：默认主按钮不应带光晕
  assert.equal(t.btnShadow, 'none');
  const point = await cdp.eval(`(() => {
    const el = document.querySelector('.btn-primary');
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  })()`);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: point.x, y: point.y, button: 'none' });
  await cdp.waitFor(`getComputedStyle(document.querySelector('.btn-primary')).boxShadow !== 'none'`);
  const glow = await cdp.eval(`({
    hovered: document.querySelector('.btn-primary').matches(':hover'),
    shadow: getComputedStyle(document.querySelector('.btn-primary')).boxShadow,
  })`);
  assert.equal(glow.hovered, true);
  assert.match(glow.shadow, /rgba\(0, 229, 255, 0\.32\)/);

  await cdp.navigate(url('#/settings'));
  await cdp.waitFor(`!!document.querySelector('.settings-card')`);
  const c = await cdp.eval(`(() => {
    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
    const probe = document.createElement('div');
    probe.innerHTML =
      '<input class="input" aria-label="probe">' +
      '<textarea class="textarea" aria-label="probe"></textarea>' +
      '<select class="select" aria-label="probe"><option>probe</option></select>' +
      '<div class="search-box"><input aria-label="probe"></div>' +
      '<div class="theme-menu-option">probe</div>' +
      '<input class="input error" aria-label="probe error">' +
      '<textarea class="textarea error" aria-label="probe error"></textarea>';
    probe.style.cssText = 'position:fixed;left:-9999px;top:0;width:300px';
    document.querySelector('.page').append(probe);
    const border = (selector) => style(selector).borderTopColor;
    const result = {
      cardRadius: style('.settings-card').borderRadius,
      corners: style('.settings-card', '::after').backgroundImage,
      cornersOpacity: style('.settings-card', '::after').opacity,
      texture: style('.settings-card', '::before').backgroundImage,
      optionRadius: style('.theme-option').borderRadius,
      optionBorder: border('.theme-option'),
      controls: {
        input: border('.input'),
        textarea: border('.textarea'),
        select: border('.select'),
        secondary: border('.btn-secondary'),
        search: border('.search-box input'),
        menuOption: border('.theme-menu-option'),
      },
      errors: {
        input: border('.input.error'),
        textarea: border('.textarea.error'),
      },
    };
    probe.remove();
    return result;
  })()`);
  assert.equal(c.cardRadius, '10px');
  assert.match(c.corners, /linear-gradient/);
  assert.equal(c.cornersOpacity, '0.5');
  assert.equal(c.texture, 'none');
  assert.equal(c.optionRadius, '8px');
  assert.equal(c.optionBorder, 'rgba(0, 229, 255, 0.55)');
  for (const [name, value] of Object.entries(c.controls)) {
    assert.equal(value, 'rgba(0, 229, 255, 0.55)', `${name} border uses --border-strong`);
  }
  // 校验错误态：强边框不得吞掉 --danger 错误边界
  assert.equal(c.errors.input, 'rgb(255, 102, 128)');
  assert.equal(c.errors.textarea, 'rgb(255, 102, 128)');

  // 主题菜单选中项：.theme-menu-option (0,2,0) 与 .theme-menu-option.active (0,2,0) 打平，
  // 若不显式提升权重，选中项会丢掉主色边界
  await cdp.eval(`document.querySelector('#themeToggle').click()`);
  await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
  const menu = await cdp.eval(`(() => {
    const active = document.querySelector('#console-theme-menu .theme-menu-option.active');
    const idle = document.querySelector('#console-theme-menu .theme-menu-option:not(.active)');
    return {
      activeBorder: getComputedStyle(active).borderTopColor,
      idleBorder: getComputedStyle(idle).borderTopColor,
      activeShadow: getComputedStyle(active).boxShadow,
    };
  })()`);
  assert.equal(menu.activeBorder, 'rgb(0, 229, 255)');
  assert.equal(menu.idleBorder, 'rgba(0, 229, 255, 0.55)');
  assert.match(menu.activeShadow, /inset/);
  await cdp.press('Escape', 'Escape', 27);
  await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);

  // HUD 四角括号覆盖：.server-card（仅总览页渲染）与 .modal（添加服务器向导）
  await cdp.navigate(url('#/overview'));
  await cdp.waitFor(`!!document.querySelector('.server-card')`);
  const corners = await cdp.eval(`(() => {
    const after = (selector) => getComputedStyle(document.querySelector(selector), '::after');
    return {
      card: after('.server-card').backgroundImage,
      cardOpacity: after('.server-card').opacity,
    };
  })()`);
  assert.match(corners.card, /linear-gradient/);
  assert.equal(corners.cardOpacity, '0.5');

  // 全局搜索焦点态：科技包的 .search-box input 强边框与 .search-box input:focus 同为 (0,2,1)，
  // 后者靠源序落败，不补规则时焦点只剩 12% 的浅青光环。
  // headless 下需开启焦点模拟，且 border/shadow 有过渡动画，读取前需等待稳定。
  await cdp.send('Emulation.setFocusEmulationEnabled', { enabled: true });
  const search = await cdp.eval(`(async () => {
    const el = document.querySelector('.search-box input');
    el.blur();
    await new Promise((r) => setTimeout(r, 200));
    const restingBorder = getComputedStyle(el).borderTopColor;
    const restingShadow = getComputedStyle(el).boxShadow;
    el.focus();
    await new Promise((r) => setTimeout(r, 260));
    const focused = getComputedStyle(el);
    return {
      focused: el.matches(':focus'),
      border: focused.borderTopColor,
      shadow: focused.boxShadow,
      restingBorder,
      restingShadow,
    };
  })()`);
  assert.equal(search.focused, true);
  assert.equal(search.restingBorder, 'rgba(0, 229, 255, 0.55)');
  assert.equal(search.border, 'rgb(0, 229, 255)');
  assert.notEqual(search.shadow, search.restingShadow);
  await cdp.eval(`document.activeElement instanceof HTMLElement && document.activeElement.blur()`);
  await cdp.send('Emulation.setFocusEmulationEnabled', { enabled: false });

  await cdp.eval(`document.querySelector('[data-action="open-wizard"]').click()`);
  await cdp.waitFor(`!!document.querySelector('.modal')`);
  const modal = await cdp.eval(`(() => {
    const after = getComputedStyle(document.querySelector('.modal'), '::after');
    return { background: after.backgroundImage, content: after.content };
  })()`);
  assert.match(modal.background, /linear-gradient/);
  assert.equal(modal.content, '""');
  await cdp.eval(`document.querySelector('.modal [data-action="close-modal"]').click()`);
  await cdp.waitFor(`!document.querySelector('.modal')`);

  // 控件边界在三种科技底色上都需达到 3:1
  const channel = (v) => {
    const x = v / 255;
    return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  };
  const luminance = ([r, g, b]) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
  const contrast = (a, b) => {
    const hi = Math.max(luminance(a), luminance(b));
    const lo = Math.min(luminance(a), luminance(b));
    return (hi + 0.05) / (lo + 0.05);
  };
  const hex = (value) => [1, 3, 5].map((i) => parseInt(value.slice(i, i + 2), 16));
  const [br, bgG, bb, ba] = c.controls.input.match(/[\d.]+/g).map(Number);
  for (const surface of [t.surface, t.surface2, t.bg]) {
    const base = hex(surface);
    const composited = [br, bgG, bb].map((v, i) => Math.round(v * ba + base[i] * (1 - ba)));
    const ratio = contrast(composited, base);
    assert.ok(ratio >= 3, `border on ${surface} is ${ratio.toFixed(2)}:1`);
  }

  await seed('desktop', 'tech');
  await cdp.navigate(url('#/desktop/prod-web-01'));
  await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
  await cdp.waitFor(`!!document.querySelector('.os-window')`);
  const d = await cdp.eval(`(() => {
    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
    const shell = getComputedStyle(document.querySelector('.desktop-shell'));
    return {
      taskbar: style('.desktop-taskbar').backdropFilter,
      conn: style('.conn-bar').backdropFilter,
      hintFilter: style('.desktop-hint').backdropFilter,
      hintBg: style('.desktop-hint').backgroundColor,
      hintColor: style('.desktop-hint').color,
      wallpaper: style('.desktop-bg').backgroundImage,
      glow: style('.desktop-bg', '::after').display,
      titlebar: style('.os-titlebar').backgroundColor,
      brandOpacity: style('.desktop-brand').opacity,
      brandColor: style('.desktop-brand').color,
      text: shell.getPropertyValue('--desktop-text').trim(),
      textMuted: shell.getPropertyValue('--desktop-text-muted').trim(),
      textShadow: shell.getPropertyValue('--desktop-text-shadow').trim(),
    };
  })()`);
  assert.equal(d.taskbar, 'blur(20px) saturate(1.25)');
  assert.equal(d.conn, 'blur(20px)');
  assert.equal(d.hintFilter, 'blur(8px)');
  assert.equal(d.hintBg, 'rgba(5, 10, 17, 0.38)');
  assert.equal(d.hintColor, 'rgba(159, 196, 214, 0.72)');
  assert.match(d.wallpaper, /3, 7, 14/);
  assert.equal(d.glow, 'none');
  assert.match(d.titlebar, /10, 22, 36/);
  assert.equal(d.brandOpacity, '1');
  assert.equal(d.brandColor, 'rgba(159, 196, 214, 0.72)');
  assert.equal(d.text, '#E6F9FF');
  assert.equal(d.textMuted, 'rgba(159, 196, 214, 0.72)');
  assert.equal(d.textShadow, '0 1px 4px rgba(0, 0, 0, 0.6)');
  assert.deepEqual(cdp.pageErrors, []);
});
