import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { extname, join, normalize, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';

const PROTOTYPE_DIR = resolve(import.meta.dirname, '..', '..', 'docs', 'prototype');
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.png': 'image/png',
  '.json': 'application/json; charset=utf-8',
};
const BROWSERS = [
  process.env.CHROME_PATH,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
].filter(Boolean);

export async function startStaticServer(dir = PROTOTYPE_DIR) {
  const rootDir = resolve(dir);
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, 'http://127.0.0.1');
      let pathname = decodeURIComponent(url.pathname);
      if (pathname === '/') pathname = '/index.html';
      const file = resolve(rootDir, '.' + normalize(pathname));
      if (file !== rootDir && !file.startsWith(rootDir + sep)) {
        res.writeHead(403);
        res.end();
        return;
      }
      const body = await readFile(file);
      res.writeHead(200, {
        'content-type': MIME[extname(file)] || 'application/octet-stream',
        'cache-control': 'no-store',
      });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  return { server, origin: `http://127.0.0.1:${server.address().port}` };
}

class CDP {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    this.requests = [];
    this.pageErrors = [];
    ws.addEventListener('message', (event) => {
      const text = typeof event.data === 'string' ? event.data : Buffer.from(event.data).toString();
      const msg = JSON.parse(text);
      if (msg.id) {
        const pending = this.pending.get(msg.id);
        if (!pending) return;
        this.pending.delete(msg.id);
        if (msg.error) pending.reject(new Error(`${msg.error.message} (${msg.error.code})`));
        else pending.resolve(msg.result);
        return;
      }
      for (const fn of this.listeners.get(msg.method) || []) fn(msg.params);
    });
    this.on('Network.requestWillBeSent', (params) => {
      if (!params.request.url.startsWith('data:')) this.requests.push(params.request.url);
    });
    this.on('Runtime.exceptionThrown', (params) => {
      const details = params.exceptionDetails;
      this.pageErrors.push(details?.exception?.description || details?.text || 'Runtime exception');
    });
    this.on('Log.entryAdded', (params) => {
      if (params.entry?.level === 'error') this.pageErrors.push(params.entry.text);
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  on(method, fn) {
    if (!this.listeners.has(method)) this.listeners.set(method, new Set());
    const set = this.listeners.get(method);
    set.add(fn);
    return () => set.delete(fn);
  }

  once(method, timeout = 10000) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        off();
        reject(new Error(`CDP timeout: ${method}`));
      }, timeout);
      const off = this.on(method, (params) => {
        clearTimeout(timer);
        off();
        resolve(params);
      });
    });
  }

  async eval(expression) {
    const result = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
    }
    return result.result.value;
  }

  async waitFor(expression, timeout = 5000) {
    const started = Date.now();
    while (Date.now() - started < timeout) {
      if (await this.eval(expression)) return true;
      await delay(50);
    }
    throw new Error(`waitFor timeout: ${expression}`);
  }

  async navigate(url) {
    // Hash-only (and identical) navigations are same-document: they never fire
    // Page.loadEventFired. Force a document load so callers always get a fresh boot.
    const current = await this.eval('location.href').catch(() => '');
    const documentOf = (href) => href.split('#')[0];
    if (current && documentOf(current) === documentOf(url)) {
      if (current !== url) {
        await this.send('Page.navigate', { url });
        await this.waitFor(`location.href === ${JSON.stringify(url)}`, 5000);
      }
      const loaded = this.once('Page.loadEventFired', 15000);
      await this.send('Page.reload', { ignoreCache: true });
      await loaded;
      return;
    }
    const loaded = this.once('Page.loadEventFired', 15000);
    await this.send('Page.navigate', { url });
    await loaded;
  }

  async reload() {
    const loaded = this.once('Page.loadEventFired', 15000);
    await this.send('Page.reload', { ignoreCache: true });
    await loaded;
  }

  async setViewport(width, height) {
    await this.send('Emulation.setDeviceMetricsOverride', {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: width < 720,
    });
  }

  async setMedia(features) {
    await this.send('Emulation.setEmulatedMedia', { media: '', features });
  }

  async addInitScript(source) {
    const result = await this.send('Page.addScriptToEvaluateOnNewDocument', { source });
    return result.identifier;
  }

  async removeInitScript(identifier) {
    await this.send('Page.removeScriptToEvaluateOnNewDocument', { identifier });
  }

  async blockUrls(urls) {
    await this.send('Network.setBlockedURLs', { urls });
  }

  async press(key, code = key, keyCode = 0, modifiers = 0) {
    await this.send('Input.dispatchKeyEvent', {
      type: 'keyDown',
      key,
      code,
      modifiers,
      windowsVirtualKeyCode: keyCode,
      nativeVirtualKeyCode: keyCode,
    });
    await this.send('Input.dispatchKeyEvent', {
      type: 'keyUp',
      key,
      code,
      modifiers,
      windowsVirtualKeyCode: keyCode,
      nativeVirtualKeyCode: keyCode,
    });
  }

  async click(x, y) {
    await this.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
    await this.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  }

  async screenshot(path, clip) {
    const result = await this.send('Page.captureScreenshot', clip
      ? { format: 'png', captureBeyondViewport: true, clip: { ...clip, scale: 1 } }
      : { format: 'png', captureBeyondViewport: false });
    await writeFile(path, Buffer.from(result.data, 'base64'));
  }

  close() {
    try { this.ws.close(); } catch {}
  }
}

function findBrowser() {
  const hit = BROWSERS.find((file) => existsSync(file));
  if (!hit) throw new Error('未找到 Chrome 或 Edge，请设置 CHROME_PATH');
  return hit;
}

export async function launchBrowser({ width = 1440, height = 900, serveDir } = {}) {
  const { server, origin } = await startStaticServer(serveDir);
  const profile = await mkdtemp(join(tmpdir(), 'lg-chrome-'));
  const proc = spawn(findBrowser(), [
    '--headless=new',
    '--disable-gpu',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--disable-background-networking',
    '--disable-component-update',
    '--hide-scrollbars',
    '--force-device-scale-factor=1',
    '--remote-debugging-port=0',
    `--user-data-dir=${profile}`,
    'about:blank',
  ], { stdio: 'ignore', windowsHide: true });

  const portFile = join(profile, 'DevToolsActivePort');
  let port = null;
  for (let i = 0; i < 100; i += 1) {
    if (existsSync(portFile)) {
      port = (await readFile(portFile, 'utf8')).split('\n')[0].trim();
      break;
    }
    await delay(100);
  }
  if (!port) {
    proc.kill();
    throw new Error('Chrome DevTools 端口未就绪');
  }

  const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: 'PUT' })).json();
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', reject, { once: true });
  });

  const cdp = new CDP(ws);
  await cdp.send('Page.enable');
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Log.enable');
  await cdp.setViewport(width, height);

  return {
    cdp,
    origin,
    close: async () => {
      try { await cdp.send('Browser.close'); } catch {}
      cdp.close();
      try { proc.kill(); } catch {}
      server.closeAllConnections?.();
      await new Promise((resolve) => server.close(resolve));
      await delay(100);
      await rm(profile, { recursive: true, force: true }).catch(() => {});
    },
  };
}
