import { mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { launchBrowser } from './cdp.mjs';

const DIST = resolve(import.meta.dirname, '..', 'apps', 'desktop', 'dist');
const OUT = resolve(import.meta.dirname, '..', '..', 'docs', 'screenshots', 'lightui', 'overlays');

await mkdir(OUT, { recursive: true });

const { cdp, origin, close } = await launchBrowser({ width: 1440, height: 1000, serveDir: DIST });

const checks = [];

function scopeSelector(scope) {
  return `.lg-scope[data-lg-scope="${scope}"]`;
}

function overlayExpression(scope, selector) {
  const other = scope === 'console' ? 'desktop' : 'console';
  return `(() => {
    const scopeEl = document.querySelector(${JSON.stringify(scopeSelector(scope))});
    const otherEl = document.querySelector(${JSON.stringify(scopeSelector(other))});
    const pickVisible = (root) => [...root.querySelectorAll(${JSON.stringify(selector)})].filter((el) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0;
    });
    const visible = pickVisible(scopeEl);
    const el = visible[visible.length - 1] || null;
    const result = { visible: visible.length, otherCount: otherEl ? pickVisible(otherEl).length : -1 };
    if (!el) return result;
    const rect = el.getBoundingClientRect();
    const owner = el.closest('.lg-scope');
    return {
      ...result,
      inPortal: !!el.closest('.lg-portal'),
      scope: owner?.dataset.lgScope ?? null,
      theme: owner?.dataset.lgTheme ?? null,
      primary: getComputedStyle(el).getPropertyValue('--lg-primary').trim(),
      scopePrimary: getComputedStyle(scopeEl).getPropertyValue('--lg-primary').trim(),
      rect: {
        x: rect.x + scrollX,
        y: rect.y + scrollY,
        width: rect.width,
        height: rect.height,
      },
    };
  })()`;
}

async function waitOverlay(scope, selector, timeout = 4000) {
  const started = Date.now();
  let info = null;
  while (Date.now() - started < timeout) {
    info = await cdp.eval(overlayExpression(scope, selector));
    if (info.visible > 0) return info;
    await delay(60);
  }
  throw new Error(`overlay not visible: ${scope} ${selector} -> ${JSON.stringify(info)}`);
}

async function waitOverlayGone(scope, selector, timeout = 4000) {
  const started = Date.now();
  while (Date.now() - started < timeout) {
    const info = await cdp.eval(overlayExpression(scope, selector));
    if (info.visible === 0) return;
    await delay(60);
  }
  throw new Error(`overlay still visible: ${scope} ${selector}`);
}

async function mouseClick(scope, selector, text = null) {
  const point = await cdp.eval(`(() => {
    const scopeEl = document.querySelector(${JSON.stringify(scopeSelector(scope))});
    const candidates = [...scopeEl.querySelectorAll(${JSON.stringify(selector)})];
    const target = ${text === null ? 'candidates[0]' : `candidates.find((el) => el.textContent.trim() === ${JSON.stringify(text)})`};
    if (!target) return null;
    target.scrollIntoView({ block: 'center', inline: 'nearest' });
    const r = target.getBoundingClientRect();
    if (!r.width || !r.height) return null;
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  })()`);
  if (!point) throw new Error(`click target not found: ${scope} / ${selector} ${text ?? ''}`);
  await cdp.click(point.x, point.y);
  await delay(220);
}

async function screenshotRect(name, rect, pad = 130) {
  await delay(240);
  const clip = {
    x: Math.max(0, Math.round(rect.x) - pad),
    y: Math.max(0, Math.round(rect.y) - pad),
    width: Math.round(rect.width) + pad * 2,
    height: Math.round(rect.height) + pad * 2,
  };
  await cdp.screenshot(join(OUT, `${name}.png`), clip);
}

function assertOverlay(label, info, expectedScope, expectedTheme) {
  const failures = [];
  if (!info.visible) failures.push('not visible');
  if (!info.inPortal) failures.push('not inside .lg-portal');
  if (info.scope !== expectedScope) failures.push(`owner scope ${info.scope}`);
  if (info.theme !== expectedTheme) failures.push(`theme ${info.theme}`);
  if (info.otherCount !== 0) failures.push(`leaked into other scope (${info.otherCount})`);
  if (!info.primary) failures.push('missing --lg-primary');
  if (info.primary !== info.scopePrimary) failures.push(`primary ${info.primary} != ${info.scopePrimary}`);
  if (failures.length) throw new Error(`${label}: ${failures.join('; ')} | ${JSON.stringify(info)}`);
  checks.push(label);
  console.log(`ok  ${label}  (${Math.round(info.rect.width)}×${Math.round(info.rect.height)} @ ${Math.round(info.rect.x)},${Math.round(info.rect.y)})`);
}

try {
  await cdp.navigate(`${origin}/`);
  await cdp.waitFor(`!!document.querySelector(${JSON.stringify(scopeSelector('console'))})`);
  await delay(400);

  // console = light, desktop = pixel
  await cdp.eval(`(() => {
    const set = (scope, index) => {
      const chips = document.querySelectorAll('.lg-scope[data-lg-scope="' + scope + '"] .theme-chip');
      chips[index].click();
    };
    set('console', 2);
    set('desktop', 4);
    return true;
  })()`);
  await delay(700);

  const themes = await cdp.eval(`(() => {
    const out = {};
    for (const scope of document.querySelectorAll('.lg-scope')) {
      out[scope.dataset.lgScope] = scope.dataset.lgTheme;
    }
    return out;
  })()`);
  if (themes.console !== 'light' || themes.desktop !== 'pixel') {
    throw new Error(`theme setup failed: ${JSON.stringify(themes)}`);
  }
  console.log('ok  theme isolation  light / pixel');

  /* ---------- console overlays ---------- */
  await mouseClick('console', 'button', '更多操作');
  let info = await waitOverlay('console', '.n-dropdown');
  assertOverlay('console/light dropdown', info, 'console', 'light');
  await screenshotRect('console-light-dropdown', info.rect);

  await mouseClick('console', '.n-dropdown-option', '复制连接命令');
  await waitOverlayGone('console', '.n-dropdown');
  const menuMessage = await waitOverlay('console', '.n-message');
  assertOverlay('console/light message (dropdown select)', menuMessage, 'console', 'light');
  await waitOverlayGone('console', '.n-message', 5000);

  await mouseClick('console', 'button', 'Popover');
  info = await waitOverlay('console', '.n-popover');
  assertOverlay('console/light popover', info, 'console', 'light');
  await screenshotRect('console-light-popover', info.rect);
  await mouseClick('console', 'button', 'Popover');
  await waitOverlayGone('console', '.n-popover');

  await mouseClick('console', 'button', 'Popconfirm');
  info = await waitOverlay('console', '.n-popconfirm');
  assertOverlay('console/light popconfirm', info, 'console', 'light');
  await screenshotRect('console-light-popconfirm', info.rect);
  await mouseClick('console', '.n-popconfirm .n-button', '确认');
  await waitOverlayGone('console', '.n-popconfirm');
  const popconfirmMessage = await waitOverlay('console', '.n-message');
  assertOverlay('console/light message (popconfirm)', popconfirmMessage, 'console', 'light');
  await waitOverlayGone('console', '.n-message', 5000);

  await mouseClick('console', '.n-select .n-base-selection');
  info = await waitOverlay('console', '.n-base-select-menu');
  assertOverlay('console/light select menu', info, 'console', 'light');
  await screenshotRect('console-light-select', info.rect, 90);
  await mouseClick('console', '.n-base-select-option', '密码');
  await waitOverlayGone('console', '.n-base-select-menu');
  const selectedText = await cdp.eval(`document.querySelector('.lg-scope[data-lg-scope="console"] .n-select').innerText.trim()`);
  if (!selectedText.includes('密码')) throw new Error(`select value not applied: ${selectedText}`);
  console.log('ok  console/light select applies value');
  checks.push('console/light select applies value');

  await mouseClick('console', 'button', '校验并连接');
  info = await waitOverlay('console', '.n-message');
  assertOverlay('console/light message (form)', info, 'console', 'light');
  await screenshotRect('console-light-message', info.rect);
  await waitOverlayGone('console', '.n-message', 5000);

  await mouseClick('console', 'button', '打开 Dialog');
  info = await waitOverlay('console', '.n-dialog');
  assertOverlay('console/light dialog', info, 'console', 'light');
  await screenshotRect('console-light-dialog', info.rect);
  await mouseClick('console', '.n-dialog .n-dialog__action button', '断开');
  await waitOverlayGone('console', '.n-dialog');
  await waitOverlayGone('console', '.n-message', 5000);

  await mouseClick('console', 'button', '打开 Modal');
  info = await waitOverlay('console', '.n-modal');
  assertOverlay('console/light modal', info, 'console', 'light');
  await screenshotRect('console-light-modal', info.rect);
  await mouseClick('console', '.n-modal button', '保存');
  await waitOverlayGone('console', '.n-modal');

  /* ---------- desktop overlays ---------- */
  await mouseClick('desktop', '.win__menu');
  info = await waitOverlay('desktop', '.n-dropdown');
  assertOverlay('desktop/pixel dropdown', info, 'desktop', 'pixel');
  await screenshotRect('desktop-pixel-dropdown', info.rect);
  await mouseClick('desktop', '.win__menu');
  await waitOverlayGone('desktop', '.n-dropdown');

  const dock = await cdp.eval(`(() => {
    const el = document.querySelector('.lg-scope[data-lg-scope="desktop"] .desk__dock-btn');
    el.scrollIntoView({ block: 'center' });
    const r = el.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  })()`);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: dock.x, y: dock.y });
  info = await waitOverlay('desktop', '.n-tooltip');
  assertOverlay('desktop/pixel tooltip', info, 'desktop', 'pixel');
  await screenshotRect('desktop-pixel-tooltip', info.rect, 80);
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 4, y: 4 });
  await waitOverlayGone('desktop', '.n-tooltip');

  await mouseClick('desktop', 'button', '重启服务');
  info = await waitOverlay('desktop', '.n-popconfirm');
  assertOverlay('desktop/pixel popconfirm', info, 'desktop', 'pixel');
  await screenshotRect('desktop-pixel-popconfirm', info.rect);
  await mouseClick('desktop', '.n-popconfirm .n-button', '确认');
  await waitOverlayGone('desktop', '.n-popconfirm');
  const desktopMessage = await waitOverlay('desktop', '.n-message');
  assertOverlay('desktop/pixel message', desktopMessage, 'desktop', 'pixel');
  await waitOverlayGone('desktop', '.n-message', 5000);

  await mouseClick('desktop', 'button', 'Modal');
  info = await waitOverlay('desktop', '.n-modal');
  assertOverlay('desktop/pixel modal', info, 'desktop', 'pixel');
  await screenshotRect('desktop-pixel-modal', info.rect);
  await mouseClick('desktop', '.n-modal button', '保存');
  await waitOverlayGone('desktop', '.n-modal');

  await mouseClick('desktop', 'button', 'Dialog');
  info = await waitOverlay('desktop', '.n-dialog');
  assertOverlay('desktop/pixel dialog', info, 'desktop', 'pixel');
  await screenshotRect('desktop-pixel-dialog', info.rect);
  await mouseClick('desktop', '.n-dialog .n-dialog__action button', '断开');
  await waitOverlayGone('desktop', '.n-dialog');
  await waitOverlayGone('desktop', '.n-message', 5000);

  /* ---------- desktop tabs ---------- */
  const winRect = await cdp.eval(`(() => {
    const el = document.querySelector('.lg-scope[data-lg-scope="desktop"] .win');
    el.scrollIntoView({ block: 'center' });
    const r = el.getBoundingClientRect();
    return { x: r.x + scrollX, y: r.y + scrollY, width: r.width, height: r.height };
  })()`);

  await mouseClick('desktop', '.n-tabs-tab', '进程');
  await delay(400);
  const processPane = await cdp.eval(`(() => {
    const body = document.querySelector('.lg-scope[data-lg-scope="desktop"] .win__body');
    return { text: body.innerText, rows: body.querySelectorAll('.n-data-table-tr').length };
  })()`);
  if (!processPane.text.includes('nginx') || processPane.rows < 4) {
    throw new Error(`process tab not rendered: ${JSON.stringify(processPane)}`);
  }
  await screenshotRect('desktop-pixel-tab-process', winRect, 40);

  await mouseClick('desktop', '.n-tabs-tab', '终端');
  await delay(400);
  const terminalText = await cdp.eval(`document.querySelector('.lg-scope[data-lg-scope="desktop"] .win__body').innerText`);
  if (!terminalText.includes('uptime')) throw new Error(`terminal tab not rendered: ${terminalText}`);
  await screenshotRect('desktop-pixel-tab-terminal', winRect, 40);
  console.log('ok  desktop tabs 进程 / 终端');
  checks.push('desktop tabs 进程 / 终端');

  /* ---------- reduced motion ---------- */
  await cdp.setMedia([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector(${JSON.stringify(scopeSelector('console'))})`);
  await delay(500);
  const motion = await cdp.eval(`(() => {
    const elements = [...document.querySelectorAll('.lg-motion')];
    const styles = elements.map((el) => {
      const style = getComputedStyle(el);
      return { transform: style.transform, opacity: style.opacity };
    });
    const animations = document.getAnimations().filter((animation) => {
      const target = animation.effect && animation.effect.target;
      return target && target.classList && target.classList.contains('lg-motion');
    });
    return { reduced: matchMedia('(prefers-reduced-motion: reduce)').matches, count: elements.length, styles, animations: animations.length };
  })()`);
  if (!motion.reduced) throw new Error('reduced-motion media not applied');
  if (!motion.count) throw new Error('no LgMotion elements found');
  const moving = motion.styles.filter((style) => style.transform !== 'none' || style.opacity !== '1');
  if (moving.length) throw new Error(`reduced-motion still offset: ${JSON.stringify(moving)}`);
  console.log(`ok  reduced-motion fallback  (${motion.count} nodes, 0 offset, active animations ${motion.animations})`);
  checks.push('reduced-motion fallback');

  await cdp.setMedia([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);

  const external = cdp.requests.filter((url) => !url.startsWith(origin) && !url.startsWith('data:'));
  console.log(JSON.stringify({ checks, pageErrors: cdp.pageErrors, externalRequests: external }, null, 2));

  if (cdp.pageErrors.length) throw new Error(`page errors: ${cdp.pageErrors.join(' | ')}`);
  if (external.length) throw new Error(`external requests: ${external.join(', ')}`);
  console.log(`\n${checks.length} checks passed`);
} finally {
  await close();
}
