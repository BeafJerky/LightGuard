import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { launchBrowser } from './cdp.mjs';

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
  await cdp.setViewport(1440, 900);
  await cdp.navigate(url(hash));
  await cdp.waitFor(`!!document.querySelector('.app')`);
}

async function seed(scope, id) {
  await open('#/overview');
  await cdp.eval(`localStorage.setItem('lightguard.theme.${scope}', ${JSON.stringify(id)})`);
  await cdp.reload();
  await cdp.waitFor(`!!document.querySelector('.app')`);
}

async function mountPressProbe() {
  return cdp.eval(`(() => {
    document.querySelector('#lgPressProbe')?.remove();
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.id = 'lgPressProbe';
    btn.className = 'btn btn-primary';
    btn.textContent = 'probe';
    btn.style.cssText = 'position:fixed;left:40px;top:40px;width:180px;height:44px;z-index:2147483647';
    document.body.append(btn);
    const rect = btn.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  })()`);
}

async function pressedTransform(point) {
  await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: point.x, y: point.y });
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x: point.x,
    y: point.y,
    button: 'left',
    buttons: 1,
    clickCount: 1,
  });
  await delay(250);
  const transform = await cdp.eval(`getComputedStyle(document.querySelector('#lgPressProbe')).transform`);
  await cdp.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x: point.x,
    y: point.y,
    button: 'left',
    buttons: 0,
    clickCount: 1,
  });
  await delay(50);
  return transform;
}

const durationMs = (value) =>
  Math.max(
    ...String(value)
      .split(',')
      .map((part) => {
        const match = /^([\d.]+(?:e[-+]?\d+)?)(ms|s)$/i.exec(part.trim());
        return match ? Number(match[1]) * (match[2] === 's' ? 1000 : 1) : Number.NaN;
      }),
  );

test('reduced motion disables pulse and the pixel/nes press offset', async () => {
  // Pin the starting media state so the first measurement is never inherited from another run.
  await cdp.setMedia([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
  for (const id of ['pixel', 'nes']) {
    await seed('console', id);
    const point = await mountPressProbe();

    try {
      const moving = await pressedTransform(point);
      assert.equal(moving, 'matrix(1, 0, 0, 1, 2, 2)', `${id}: press offset applies while motion is allowed`);

      await cdp.setMedia([{ name: 'prefers-reduced-motion', value: 'reduce' }]);
      const still = await pressedTransform(point);
      assert.equal(still, 'none', `${id}: pack press offset is cancelled under reduced motion`);

      const reduced = await cdp.eval(`(() => {
        const dot = document.createElement('span');
        dot.className = 'status-dot';
        document.body.append(dot);
        const after = getComputedStyle(dot, '::after');
        const out = {
          display: after.display,
          duration: getComputedStyle(document.querySelector('#lgPressProbe')).transitionDuration,
        };
        dot.remove();
        return out;
      })()`);
      assert.equal(reduced.display, 'none', `${id}: status pulse is hidden`);
      // Chrome reports the collapsed duration as "1e-05s", older builds as "0.01ms"
      assert.ok(durationMs(reduced.duration) <= 0.1, `${id}: transitions collapse (got ${reduced.duration})`);
    } finally {
      await cdp.setMedia([{ name: 'prefers-reduced-motion', value: 'no-preference' }]);
      await cdp.eval(`document.querySelector('#lgPressProbe')?.remove()`);
    }
  }
});

async function enableForcedColors() {
  const active = `matchMedia('(forced-colors: active)').matches`;
  await cdp.setMedia([{ name: 'forced-colors', value: 'active' }]);
  if (await cdp.eval(active)) return 'features';
  await cdp.setMedia([]);
  await cdp.send('Emulation.setEmulatedMedia', {
    media: '',
    features: [{ name: 'forced-colors', value: 'active' }],
    forcedColors: 'active',
  });
  if (await cdp.eval(active)) return 'features+forcedColors';
  return null;
}

test('forced colors removes textures and keeps borders and selection visible', async () => {
  // pixel uses --border-width: 2px, so a computed 1px border proves the forced rule wins
  await seed('console', 'pixel');
  let emulated = null;
  try {
    emulated = await enableForcedColors();
    assert.notEqual(emulated, null, 'forced-colors media emulation is unsupported in this Chrome');

    // second forced-colors declaration, independent of the card border
    const dotAdjust = await cdp.eval(`(() => {
      let dot = document.querySelector('.status-dot');
      const created = !dot;
      if (!dot) {
        dot = document.createElement('span');
        dot.className = 'status-dot';
        document.body.append(dot);
      }
      const value = getComputedStyle(dot).forcedColorAdjust;
      if (created) dot.remove();
      return value;
    })()`);
    assert.equal(dotAdjust, 'none', 'status dot keeps its own colours under forced colors');

    await cdp.navigate(url('#/settings'));
    await cdp.waitFor(`!!document.querySelector('.settings-card')`);

    const result = await cdp.eval(`(() => {
      const card = document.querySelector('.settings-card');
      const s = getComputedStyle(card);
      const body = getComputedStyle(document.body, '::before');
      const texture = getComputedStyle(card, '::before');
      return {
        borderStyle: s.borderTopStyle,
        borderWidth: s.borderTopWidth,
        bodyImage: body.backgroundImage,
        bodyDisplay: body.display,
        textureDisplay: texture.display,
      };
    })()`);
    assert.equal(result.borderStyle, 'solid');
    assert.equal(result.borderWidth, '1px');
    assert.equal(result.bodyImage, 'none');
    assert.equal(result.bodyDisplay, 'none');
    assert.equal(result.textureDisplay, 'none');

    await cdp.waitFor(`!!document.querySelector('[data-theme-group="console"] .theme-option.active')`);
    const outline = await cdp.eval(`(() => {
      const s = getComputedStyle(document.querySelector('[data-theme-group="console"] .theme-option.active'));
      return { style: s.outlineStyle, width: s.outlineWidth };
    })()`);
    assert.equal(outline.style, 'solid');
    assert.equal(outline.width, '2px');
  } finally {
    // Runs even when enableForcedColors() or the support assertion throws, so the emulated
    // palette can never leak into later cases.
    await cdp.setMedia([]);
    if (emulated !== 'features') {
      // the fallback payload also switched the emulated forced-colors palette
      await cdp
        .send('Emulation.setEmulatedMedia', { media: '', features: [], forcedColors: 'none' })
        .catch(() => {});
    }
  }
});

test('input placeholder renders at full opacity', async () => {
  await seed('console', 'light');
  const result = await cdp.eval(`(() => {
    const input = document.createElement('input');
    input.className = 'input';
    input.placeholder = 'placeholder probe';
    input.style.cssText = 'position:fixed;left:40px;top:40px;width:200px';
    document.body.append(input);
    const placeholder = getComputedStyle(input, '::placeholder');
    const out = { opacity: placeholder.opacity };
    input.remove();
    return out;
  })()`);
  assert.equal(result.opacity, '1', 'placeholder is not dimmed by opacity');
});

test('every pack keeps a visible keyboard focus ring', async () => {
  for (const id of ['dark', 'light', 'claude', 'pixel', 'nes', 'tech']) {
    await seed('console', id);
    await cdp.navigate(url('#/settings'));
    const selector = `[data-theme-group="console"] input[data-theme-option][value="${id}"]`;
    await cdp.waitFor(`!!document.querySelector('${selector}')`);
    assert.equal(
      await cdp.eval(`(() => {
        const input = document.querySelector('${selector}');
        input.focus();
        return document.activeElement === input;
      })()`),
      true,
      `${id}: radio receives focus`,
    );
    const ring = await cdp.eval(`(() => {
      const option = document.querySelector('${selector}').closest('.theme-option');
      const s = getComputedStyle(option);
      return { style: s.outlineStyle, width: s.outlineWidth };
    })()`);
    assert.equal(ring.style, 'solid', `${id}: outline style`);
    assert.equal(ring.width, '2px', `${id}: outline width`);
  }
});

test('mobile settings and theme sheet do not overflow horizontally', async () => {
  await seed('console', 'claude');
  await cdp.setViewport(390, 844);
  try {
    assert.equal(await cdp.eval('window.innerWidth'), 390, 'viewport emulated at 390px');
    await cdp.navigate(url('#/settings'));
    await cdp.waitFor(`!!document.querySelector('[data-theme-group="console"]')`);
    assert.equal(await cdp.eval(`document.documentElement.scrollWidth <= window.innerWidth`), true);

    await cdp.eval(`document.querySelector('[data-action="open-theme-menu"][data-theme-scope="console"]').click()`);
    await cdp.waitFor(`!!document.querySelector('#console-theme-menu')`);
    const sheet = await cdp.eval(`(() => {
      const panel = document.querySelector('#console-theme-menu');
      const rect = panel.getBoundingClientRect();
      return {
        width: rect.width,
        left: rect.left,
        right: rect.right,
        scrollWidth: document.documentElement.scrollWidth,
        inner: window.innerWidth,
      };
    })()`);
    assert.ok(sheet.width <= 390, `sheet width ${sheet.width}`);
    assert.ok(sheet.left >= 0 && sheet.right <= 390, `sheet bounds ${sheet.left}..${sheet.right}`);
    assert.ok(sheet.scrollWidth <= sheet.inner, `scrollWidth ${sheet.scrollWidth} <= ${sheet.inner}`);
    await cdp.press('Escape', 'Escape', 27);
    await cdp.waitFor(`!document.querySelector('#console-theme-menu')`);
  } finally {
    await cdp.setViewport(1440, 900);
  }
});

test('all themes and routes make no external requests', async () => {
  cdp.requests.length = 0;
  cdp.pageErrors.length = 0;
  const perTheme = {};
  for (const id of ['dark', 'light', 'system', 'claude', 'pixel', 'nes', 'tech']) {
    const before = cdp.requests.length;
    await seed('console', id);
    await cdp.navigate(url('#/settings'));
    await cdp.waitFor(`!!document.querySelector('[data-theme-group="console"]')`);
    await cdp.navigate(url('#/desktop/prod-web-01'));
    await cdp.waitFor(`!!document.querySelector('#desktopShell')`);
    perTheme[id] = cdp.requests.length - before;
  }
  // positive control: the recorder must actually observe traffic, otherwise "no external" is vacuous
  assert.ok(cdp.requests.length > 0, 'request recorder captured traffic');
  for (const [id, count] of Object.entries(perTheme)) {
    assert.ok(count > 0, `${id}: requests were recorded (${count})`);
  }
  const external = cdp.requests.filter(
    (request) => !request.startsWith(origin) && !request.startsWith('data:') && !request.startsWith('blob:'),
  );
  assert.deepEqual(external, []);
  assert.deepEqual(cdp.pageErrors, []);
});
